use std::collections::HashMap;
use std::sync::Arc;

use composer_protocol::{
    ClientCommand, ClientRequest, CommandResult, ServerMessage, ToolInfo, WorkerState,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use crate::tools::CallResult;
use crate::worker::types::Outbound;

/// A worker process's connection to the server that spawned it. Through it the worker
/// borrows model slots, asks for approvals, and reports its session's events.
#[derive(Debug)]
pub struct ParentLink {
    tx: mpsc::UnboundedSender<Outbound>,
    closed: Arc<Notify>,
}

impl ParentLink {
    /// Connects as the worker process running `session`.
    pub async fn connect(
        parent_url: &str,
        token: &str,
        session: &str,
    ) -> Result<Arc<Self>, String> {
        let link = Self::open(parent_url, token).await?;
        link.request(ClientCommand::WorkerHello {
            session: session.to_string(),
        })
        .await?;
        Ok(link)
    }

    /// Connects without announcing a session (a harness's tool server).
    pub async fn open(parent_url: &str, token: &str) -> Result<Arc<Self>, String> {
        let ws_url = format!(
            "{}/ws?token={token}",
            parent_url.trim_end_matches('/').replacen("http", "ws", 1)
        );
        let (socket, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|e| format!("connecting to the parent: {e}"))?;
        let (mut sink, mut stream) = socket.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Outbound>();
        let closed = Arc::new(Notify::new());

        let on_close = closed.clone();
        tokio::spawn(async move {
            let mut next_id = 0u64;
            let mut waiting: HashMap<u64, oneshot::Sender<Result<CommandResult, String>>> =
                HashMap::new();
            loop {
                tokio::select! {
                    outbound = rx.recv() => {
                        let Some(Outbound { command, reply }) = outbound else { break };
                        next_id += 1;
                        if let Some(reply) = reply {
                            waiting.insert(next_id, reply);
                        }
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
                            _ => {}
                        }
                    }
                }
            }
            for (_, reply) in waiting {
                let _ = reply.send(Err("lost the connection to the parent".into()));
            }
            on_close.notify_waiters();
            on_close.notify_one();
        });

        Ok(Arc::new(Self { tx, closed }))
    }

    async fn request(&self, command: ClientCommand) -> Result<CommandResult, String> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(Outbound {
                command,
                reply: Some(reply),
            })
            .map_err(|_| "lost the connection to the parent".to_string())?;
        answer
            .await
            .map_err(|_| "lost the connection to the parent".to_string())?
    }

    /// Reports an event of the worker's session (no reply).
    pub fn event(&self, event: ServerMessage) {
        let _ = self.tx.send(Outbound {
            command: ClientCommand::WorkerEvent {
                event: Box::new(event),
            },
            reply: None,
        });
    }

    /// Waits until the parent grants a model slot.
    pub async fn acquire_slot(&self) -> Result<u64, String> {
        match self.request(ClientCommand::SlotAcquire).await? {
            CommandResult::SlotGranted { grant } => Ok(grant),
            other => Err(format!("unexpected reply to slot.acquire: {other:?}")),
        }
    }

    pub fn release_slot(&self, grant: u64) {
        let _ = self.tx.send(Outbound {
            command: ClientCommand::SlotRelease { grant },
            reply: None,
        });
    }

    /// Asks the user, through the parent, to approve a recipe version.
    pub async fn ask_approval(
        &self,
        site: &str,
        recipe: &str,
        args: &[String],
        body: String,
        hash: &str,
    ) -> Result<(bool, Option<String>), String> {
        let command = ClientCommand::ApprovalAsk {
            site: site.into(),
            recipe: recipe.into(),
            args: args.to_vec(),
            body,
            hash: hash.into(),
        };
        match self.request(command).await? {
            CommandResult::Approval { approved, reason } => Ok((approved, reason)),
            other => Err(format!("unexpected reply to approval.ask: {other:?}")),
        }
    }

    pub async fn finish(&self, state: WorkerState, summary: String) -> Result<(), String> {
        self.request(ClientCommand::WorkerFinish { state, summary })
            .await
            .map(|_| ())
    }

    /// At the turn limit: waits until the orchestrator continues (its message) or stops (an
    /// error) this worker.
    /// The reply: the message, and whether to compact first.
    pub async fn stuck(&self, summary: String) -> Result<(String, bool), String> {
        match self.request(ClientCommand::WorkerStuck { summary }).await? {
            CommandResult::WorkerGoOn { message, compact } => Ok((message, compact)),
            other => Err(format!("unexpected reply to worker.stuck: {other:?}")),
        }
    }

    /// The tools the running worker `session` may call.
    pub async fn tools(&self, session: &str) -> Result<Vec<ToolInfo>, String> {
        match self
            .request(ClientCommand::ToolList {
                session: session.into(),
            })
            .await?
        {
            CommandResult::Tools { tools } => Ok(tools),
            other => Err(format!("unexpected reply to tool.list: {other:?}")),
        }
    }

    /// Runs a tool in the running worker `session`: its output, whether it failed, its images.
    pub async fn call_tool(
        &self,
        session: &str,
        name: &str,
        args: serde_json::Value,
    ) -> Result<CallResult, String> {
        let command = ClientCommand::ToolCall {
            session: session.into(),
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

    /// Resolves when the connection to the parent is gone.
    pub async fn closed(&self) {
        self.closed.notified().await;
    }
}
