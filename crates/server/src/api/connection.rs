use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use composer_protocol::{
    ChatMessage, ClientCommand, ClientRequest, CommandResult, MessageSource, Role, Seq,
    ServerMessage, SessionId,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent::AgentDefinition;
use crate::api::event_scope::{EventScope, scope};
use crate::api::glob::url_matches;
use crate::api::resequence::resequence;
use crate::server::Server;
use crate::session::{compact_now, submit};
use crate::slot_guard::SlotGuard;
use crate::tools::resolve_project;

/// One client's WebSocket: what it attached to and subscribed to.
pub struct Connection {
    out: mpsc::UnboundedSender<Arc<ServerMessage>>,
    /// Attached sessions → last seq already sent (replay or live).
    attached: Mutex<HashMap<SessionId, Seq>>,
    subscriptions: Mutex<Vec<String>>,
    /// Set when the other side is a worker process: the session it runs.
    worker_session: Mutex<Option<SessionId>>,
    /// Model slots lent to a worker; dropped (released) with the connection.
    grants: Mutex<HashMap<u64, SlotGuard>>,
    next_grant: AtomicU64,
}

impl Connection {
    pub fn new(out: mpsc::UnboundedSender<Arc<ServerMessage>>) -> Self {
        Self {
            out,
            attached: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(Vec::new()),
            worker_session: Mutex::new(None),
            grants: Mutex::new(HashMap::new()),
            next_grant: AtomicU64::new(0),
        }
    }

    pub fn send(&self, message: Arc<ServerMessage>) {
        let _ = self.out.send(message);
    }

    /// Whether a broadcast event should go to this client. Marks session events as sent.
    pub fn wants(&self, message: &ServerMessage) -> bool {
        match scope(message) {
            EventScope::Global => true,
            EventScope::Reply => false,
            EventScope::Page(url) => self
                .subscriptions
                .lock()
                .expect("subs lock")
                .iter()
                .any(|p| url_matches(p, url)),
            EventScope::Session { id, seq } => {
                let mut attached = self.attached.lock().expect("attached lock");
                match (attached.get_mut(id), seq) {
                    (Some(sent), Some(seq)) if seq > *sent => {
                        *sent = seq;
                        true
                    }
                    (Some(_), None) => true,
                    _ => false,
                }
            }
        }
    }

    /// Handles a command. Returns the reply, or `None` when there is none yet: slow
    /// commands (waiting for a slot or an approval) reply later, and worker events never do.
    pub fn handle(
        self: &Arc<Self>,
        server: &Arc<Server>,
        request: ClientRequest,
    ) -> Option<ServerMessage> {
        let id = request.id;
        match request.command {
            ClientCommand::WorkerEvent { event } => {
                self.worker_event(server, *event);
                None
            }
            ClientCommand::SlotRelease { grant } => {
                self.grants.lock().expect("grants lock").remove(&grant);
                None
            }
            ClientCommand::SlotAcquire => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let reply = match server.slots.acquire().await {
                        Ok(guard) => {
                            let grant = connection.next_grant.fetch_add(1, Ordering::SeqCst) + 1;
                            connection
                                .grants
                                .lock()
                                .expect("grants lock")
                                .insert(grant, guard);
                            ServerMessage::Ok {
                                id,
                                result: CommandResult::SlotGranted { grant },
                            }
                        }
                        Err(message) => ServerMessage::Error {
                            id: Some(id),
                            message,
                        },
                    };
                    connection.send(Arc::new(reply));
                });
                None
            }
            ClientCommand::ApprovalAsk {
                site,
                recipe,
                args,
                body,
                hash,
            } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let reply = match connection
                        .ask_approval(&server, &site, &recipe, &args, body, &hash)
                        .await
                    {
                        Ok(approved) => ServerMessage::Ok {
                            id,
                            result: CommandResult::Approval { approved },
                        },
                        Err(message) => ServerMessage::Error {
                            id: Some(id),
                            message,
                        },
                    };
                    connection.send(Arc::new(reply));
                });
                None
            }
            command => Some(match self.run(server, command) {
                Ok(result) => ServerMessage::Ok { id, result },
                Err(message) => ServerMessage::Error {
                    id: Some(id),
                    message,
                },
            }),
        }
    }

    /// An event from the worker process: re-sequenced into its session here, or its info mirrored.
    fn worker_event(&self, server: &Arc<Server>, event: ServerMessage) {
        let Some(id) = self.worker_session.lock().expect("worker lock").clone() else {
            return;
        };
        let Ok(session) = server.sessions.get(&id) else {
            return;
        };
        match event {
            ServerMessage::SessionUpdated { session: info } => {
                session.update_info(&server.hub, |mine| {
                    mine.state = info.state;
                    mine.context_tokens = info.context_tokens;
                    mine.project = info.project;
                })
            }
            event if resequence(&event, 0).is_some() => {
                session.emit(&server.hub, |seq| {
                    resequence(&event, seq).expect("checked above")
                });
            }
            _ => {}
        }
    }

    /// A worker's recipe approval: already approved here, or asked of this server's clients.
    async fn ask_approval(
        &self,
        server: &Arc<Server>,
        site: &str,
        recipe: &str,
        args: &[String],
        body: String,
        hash: &str,
    ) -> Result<bool, String> {
        if server.approvals.is_approved(site, recipe, hash) {
            return Ok(true);
        }
        let id = self
            .worker_session
            .lock()
            .expect("worker lock")
            .clone()
            .ok_or("not a worker connection")?;
        let session = server.sessions.get(&id).map_err(|e| e.to_string())?;
        let decision = server
            .request_approval(
                &session,
                site,
                recipe,
                args,
                body,
                hash,
                &CancellationToken::new(),
            )
            .await;
        let approved = decision == Some(composer_protocol::ApprovalDecision::Approve);
        if approved {
            server
                .approvals
                .approve(site, recipe, hash)
                .map_err(|e| e.to_string())?;
        }
        Ok(approved)
    }

    fn run(&self, server: &Arc<Server>, command: ClientCommand) -> Result<CommandResult, String> {
        match command {
            ClientCommand::SessionCreate { agent, project } => {
                AgentDefinition::load(&server.workspace, &agent).map_err(|e| e.to_string())?;
                let project = project
                    .as_deref()
                    .map(|p| resolve_project(p, &server.config.projects.dir))
                    .transpose()?;
                let session = server
                    .sessions
                    .create(&agent, project)
                    .map_err(|e| e.to_string())?;
                server.hub.send(ServerMessage::SessionUpdated {
                    session: session.info(),
                });
                Ok(CommandResult::Session {
                    session: session.info(),
                })
            }
            ClientCommand::SessionList => Ok(CommandResult::Sessions {
                sessions: server.sessions.list(),
            }),
            ClientCommand::SessionAttach { session, since } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                // Under the transcript lock: nothing is emitted between the replay and marking it attached.
                s.transcript()
                    .replay(since, |events, last| {
                        for event in events {
                            self.send(Arc::new(event));
                        }
                        self.attached
                            .lock()
                            .expect("attached lock")
                            .insert(session.clone(), last);
                    })
                    .map_err(|e| e.to_string())?;
                for request in server.pending_approvals.for_session(&session) {
                    self.send(Arc::new(request));
                }
                Ok(CommandResult::Session { session: s.info() })
            }
            ClientCommand::SessionDetach { session } => {
                self.attached
                    .lock()
                    .expect("attached lock")
                    .remove(&session);
                Ok(CommandResult::Done)
            }
            ClientCommand::MessageSend {
                session,
                text,
                pages,
                images,
            } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                let message = ChatMessage {
                    role: Role::User,
                    content: text,
                    thinking: None,
                    tool_calls: vec![],
                    pages,
                    images,
                    source: Some(MessageSource::Tui),
                };
                submit(server, &s, message);
                Ok(CommandResult::Done)
            }
            ClientCommand::SessionCompact { session } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                compact_now(server, &s)?;
                Ok(CommandResult::Done)
            }
            ClientCommand::TurnCancel { session } => {
                server
                    .sessions
                    .get(&session)
                    .map_err(|e| e.to_string())?
                    .cancel();
                Ok(CommandResult::Done)
            }
            ClientCommand::ProjectSet { session, path } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                let project = path
                    .as_deref()
                    .filter(|p| !p.trim().is_empty())
                    .map(|p| resolve_project(p, &server.config.projects.dir))
                    .transpose()?;
                s.update_info(&server.hub, |info| info.project = project);
                Ok(CommandResult::Session { session: s.info() })
            }
            ClientCommand::ApprovalRespond { request, decision } => {
                if server.pending_approvals.resolve(&request, decision) {
                    Ok(CommandResult::Done)
                } else {
                    Err(format!("approval request `{request}` is no longer open"))
                }
            }
            ClientCommand::WorkerHello { session } => {
                server.sessions.get(&session).map_err(|e| e.to_string())?;
                *self.worker_session.lock().expect("worker lock") = Some(session);
                Ok(CommandResult::Done)
            }
            ClientCommand::WorkerFinish { state, summary } => {
                let id = self
                    .worker_session
                    .lock()
                    .expect("worker lock")
                    .clone()
                    .ok_or("not a worker connection")?;
                server.workers.finish(server, &id, state, summary);
                Ok(CommandResult::Done)
            }
            ClientCommand::WorkerEvent { .. }
            | ClientCommand::SlotAcquire
            | ClientCommand::SlotRelease { .. }
            | ClientCommand::ApprovalAsk { .. } => Err("handled asynchronously".into()),
            ClientCommand::Subscribe { urls } => {
                *self.subscriptions.lock().expect("subs lock") = urls;
                Ok(CommandResult::Done)
            }
        }
    }
}
