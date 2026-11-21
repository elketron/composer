use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use composer_protocol::{ClientCommand, ClientRequest, CommandResult, ServerMessage, ToolInfo};
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use crate::mcp::ToolSource;
use crate::tools::CallResult;

type Reply = oneshot::Sender<Result<CommandResult, String>>;

/// The MCP bridge's connection to the running server: a session's tools, run by the server over
/// the WebSocket (`tool.list` / `tool.call`).
#[derive(Debug)]
pub struct ServerLink {
    tx: mpsc::UnboundedSender<(ClientCommand, Reply)>,
    closed: Arc<Notify>,
    /// Notified when the server says the session's tools changed (`tools.changed`).
    changed: Arc<Notify>,
    session: String,
}

impl ServerLink {
    /// Connects to the server at `url` with `token`, for the tools of `session`.
    pub async fn open(url: &str, token: &str, session: &str) -> Result<Arc<Self>, String> {
        let ws_url = format!(
            "{}/ws?token={token}",
            url.trim_end_matches('/').replacen("http", "ws", 1)
        );
        let (socket, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|e| format!("connecting to the server: {e}"))?;
        let (mut sink, mut stream) = socket.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<(ClientCommand, Reply)>();
        let closed = Arc::new(Notify::new());
        let changed = Arc::new(Notify::new());

        let on_close = closed.clone();
        let on_change = changed.clone();
        let own = session.to_string();
        tokio::spawn(async move {
            let mut next_id = 0u64;
            let mut waiting: HashMap<u64, Reply> = HashMap::new();
            loop {
                tokio::select! {
                    outbound = rx.recv() => {
                        let Some((command, reply)) = outbound else { break };
                        next_id += 1;
                        waiting.insert(next_id, reply);
                        let json = serde_json::to_string(&ClientRequest { id: next_id, command }).expect("serializes");
                        if sink.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    frame = stream.next() => {
                        let Some(Ok(frame)) = frame else { break };
                        let Message::Text(text) = frame else { continue };
                        match serde_json::from_str::<ServerMessage>(&text) {
                            Ok(ServerMessage::Ok { id, result }) => {
                                if let Some(reply) = waiting.remove(&id) {
                                    let _ = reply.send(Ok(result));
                                }
                            }
                            Ok(ServerMessage::Error { id: Some(id), message }) => {
                                if let Some(reply) = waiting.remove(&id) {
                                    let _ = reply.send(Err(message));
                                }
                            }
                            Ok(ServerMessage::ToolsChanged { session }) if session == own => {
                                on_change.notify_one();
                            }
                            _ => {}
                        }
                    }
                }
            }
            for (_, reply) in waiting {
                let _ = reply.send(Err("lost the connection to the server".into()));
            }
            on_close.notify_waiters();
            on_close.notify_one();
        });

        Ok(Arc::new(Self {
            tx,
            closed,
            changed,
            session: session.to_string(),
        }))
    }

    async fn request(&self, command: ClientCommand) -> Result<CommandResult, String> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send((command, reply))
            .map_err(|_| "lost the connection to the server".to_string())?;
        answer
            .await
            .map_err(|_| "lost the connection to the server".to_string())?
    }

    /// Resolves when the connection to the server is gone.
    pub async fn closed(&self) {
        self.closed.notified().await;
    }
}

#[async_trait]
impl ToolSource for ServerLink {
    async fn tools(&self) -> Result<Vec<ToolInfo>, String> {
        let command = ClientCommand::ToolList {
            session: self.session.clone(),
        };
        match self.request(command).await? {
            CommandResult::Tools { tools } => Ok(tools),
            other => Err(format!("unexpected reply to tool.list: {other:?}")),
        }
    }

    async fn call(&self, name: &str, args: Value) -> Result<CallResult, String> {
        let command = ClientCommand::ToolCall {
            session: self.session.clone(),
            name: name.into(),
            args,
        };
        match self.request(command).await? {
            CommandResult::ToolOutput {
                output,
                is_error,
                images,
            } => Ok(CallResult {
                text: output,
                is_error,
                images,
            }),
            other => Err(format!("unexpected reply to tool.call: {other:?}")),
        }
    }

    async fn changed(&self) {
        self.changed.notified().await;
    }
}
