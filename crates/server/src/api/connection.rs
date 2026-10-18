use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use composer_protocol::{
    ChatMessage, ClientCommand, ClientRequest, CommandResult, MessageSource, Role, Seq,
    ServerMessage, SessionId,
};
use tokio::sync::mpsc;

use crate::agent::AgentDefinition;
use crate::api::event_scope::{EventScope, scope};
use crate::api::glob::url_matches;
use crate::server::Server;
use crate::session::submit;
use crate::tools::resolve_project;

/// One client's WebSocket: what it attached to and subscribed to.
pub struct Connection {
    out: mpsc::UnboundedSender<Arc<ServerMessage>>,
    /// Attached sessions → last seq already sent (replay or live).
    attached: Mutex<HashMap<SessionId, Seq>>,
    subscriptions: Mutex<Vec<String>>,
}

impl Connection {
    pub fn new(out: mpsc::UnboundedSender<Arc<ServerMessage>>) -> Self {
        Self {
            out,
            attached: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(Vec::new()),
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

    pub fn handle(&self, server: &Arc<Server>, request: ClientRequest) -> ServerMessage {
        let id = request.id;
        match self.run(server, request.command) {
            Ok(result) => ServerMessage::Ok { id, result },
            Err(message) => ServerMessage::Error {
                id: Some(id),
                message,
            },
        }
    }

    fn run(&self, server: &Arc<Server>, command: ClientCommand) -> Result<CommandResult, String> {
        match command {
            ClientCommand::SessionCreate { agent, project } => {
                AgentDefinition::load(&server.workspace, &agent).map_err(|e| e.to_string())?;
                let project = project.as_deref().map(resolve_project).transpose()?;
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
                    .map(resolve_project)
                    .transpose()?;
                s.update_info(&server.hub, |info| info.project = project);
                Ok(CommandResult::Session { session: s.info() })
            }
            ClientCommand::ApprovalRespond { .. } => {
                Err("recipe approvals arrive in milestone 4".into())
            }
            ClientCommand::Subscribe { urls } => {
                *self.subscriptions.lock().expect("subs lock") = urls;
                Ok(CommandResult::Done)
            }
        }
    }
}
