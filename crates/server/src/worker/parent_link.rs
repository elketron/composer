use std::collections::HashMap;
use std::sync::Arc;

use composer_protocol::{ClientCommand, ClientRequest, CommandResult, ServerMessage, WorkerState};
use futures::{SinkExt, StreamExt};
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use crate::worker::types::Outbound;

/// A worker process's connection to the server that spawned it. Through it the worker
/// borrows model slots, asks for approvals, and reports its session's events.
#[derive(Debug)]
pub struct ParentLink {
    tx: mpsc::UnboundedSender<Outbound>,
    closed: Arc<Notify>,
}

impl ParentLink {
    /// Connects and announces `session`.
    pub async fn connect(
        parent_url: &str,
        token: &str,
        session: &str,
    ) -> Result<Arc<Self>, String> {
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

        let link = Arc::new(Self { tx, closed });
        link.request(ClientCommand::WorkerHello {
            session: session.to_string(),
        })
        .await?;
        Ok(link)
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
    ) -> Result<bool, String> {
        let command = ClientCommand::ApprovalAsk {
            site: site.into(),
            recipe: recipe.into(),
            args: args.to_vec(),
            body,
            hash: hash.into(),
        };
        match self.request(command).await? {
            CommandResult::Approval { approved } => Ok(approved),
            other => Err(format!("unexpected reply to approval.ask: {other:?}")),
        }
    }

    pub async fn finish(&self, state: WorkerState, summary: String) -> Result<(), String> {
        self.request(ClientCommand::WorkerFinish { state, summary })
            .await
            .map(|_| ())
    }

    /// Resolves when the connection to the parent is gone.
    pub async fn closed(&self) {
        self.closed.notified().await;
    }
}
