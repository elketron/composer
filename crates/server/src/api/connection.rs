use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use composer_protocol::{
    Attachment, ChatMessage, ClientCommand, ClientRequest, CommandResult, MessageSource, Role, Seq,
    ServerMessage, SessionId,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent::AgentDefinition;
use crate::api::event_scope::{EventScope, scope};
use crate::api::glob::url_matches;
use crate::attachments::resolve;
use crate::harness::{PERMISSION_TOOL, WorkerSink, answer_permission, permission_tool_info};
use crate::server::Server;
use crate::session::{compact_now, submit};
use crate::slot_guard::SlotGuard;
use crate::tools::resolve_project;
use crate::worker::ticket::normalize_id;

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

    /// The reply to command `id`.
    fn reply(&self, id: u64, result: Result<CommandResult, String>) {
        self.send(Arc::new(match result {
            Ok(result) => ServerMessage::Ok { id, result },
            Err(message) => ServerMessage::Error {
                id: Some(id),
                message,
            },
        }));
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
            // A stopped external worker: the message continues its run (`--resume`).
            ClientCommand::MessageSend {
                session,
                text,
                attachments,
                ..
            } if server
                .sessions
                .get(&session)
                .is_ok_and(|s| server.workers.resumable(server, &s)) =>
            {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let resumed = match (
                        server.sessions.get(&session),
                        resolve_attachments(&server, &session, &attachments),
                    ) {
                        (Ok(s), Ok(attachments)) => {
                            server.workers.resume(&server, &s, text, attachments).await
                        }
                        (Err(e), _) => Err(e.to_string()),
                        (_, Err(e)) => Err(e),
                    };
                    connection.send(Arc::new(match resumed {
                        Ok(()) => ServerMessage::Ok {
                            id,
                            result: CommandResult::Done,
                        },
                        Err(message) => ServerMessage::Error {
                            id: Some(id),
                            message,
                        },
                    }));
                });
                None
            }
            ClientCommand::HarnessSet { project, entry, on } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let result = match resolve_project(&project, &server.config.projects.dir) {
                        Ok(folder) => server
                            .workers
                            .switch(&server, &folder, &entry, on)
                            .await
                            .map(|()| CommandResult::Harnesses {
                                harnesses: server.workers.switches(&server, &folder),
                                project: folder,
                            }),
                        Err(e) => Err(e),
                    };
                    connection.send(Arc::new(match result {
                        Ok(result) => ServerMessage::Ok { id, result },
                        Err(message) => ServerMessage::Error {
                            id: Some(id),
                            message,
                        },
                    }));
                });
                None
            }
            ClientCommand::TicketChanges { ticket } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let ticket = normalize_id(&ticket);
                    let result = crate::worker::ticket_changes(&server, &ticket)
                        .await
                        .map(|changes| CommandResult::Changes { changes });
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::TicketMerge { ticket } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::task::spawn_blocking(move || {
                    let ticket = normalize_id(&ticket);
                    let result = crate::worker::merge_ticket(&server, &ticket)
                        .map(|commit| CommandResult::Merged { commit });
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::TicketDiscard { ticket } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::task::spawn_blocking(move || {
                    let ticket = normalize_id(&ticket);
                    let result = crate::worker::discard_ticket(&server, &ticket)
                        .map(|()| CommandResult::Done);
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::ToolCall {
                session,
                name,
                args,
            } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    if name == PERMISSION_TOOL
                        && server.workers.get(&session).is_some_and(|h| h.resumable)
                    {
                        let output = answer_permission(&server, &session, args).await;
                        connection.reply(
                            id,
                            Ok(CommandResult::ToolOutput {
                                output,
                                is_error: false,
                            }),
                        );
                        return;
                    }
                    let reply = match server.workers.tool_context(&server, &session) {
                        Ok(context) => {
                            let (output, is_error) =
                                server.tools.call_value(&context, &name, args).await;
                            ServerMessage::Ok {
                                id,
                                result: CommandResult::ToolOutput { output, is_error },
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

    /// An event from the worker process, for its session here.
    fn worker_event(&self, server: &Arc<Server>, event: ServerMessage) {
        let Some(id) = self.worker_session.lock().expect("worker lock").clone() else {
            return;
        };
        if let Some(sink) = WorkerSink::new(server, &id) {
            sink.event(event);
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
        let approved = matches!(
            decision,
            Some(
                composer_protocol::ApprovalDecision::Approve
                    | composer_protocol::ApprovalDecision::Always
            )
        );
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
                attachments,
            } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                let attachments = resolve_attachments(server, &session, &attachments)?;
                let message = ChatMessage {
                    role: Role::User,
                    content: text,
                    thinking: None,
                    tool_calls: vec![],
                    pages,
                    images,
                    attachments,
                    source: Some(MessageSource::Tui),
                };
                // A running worker's turns happen in its harness, not here.
                match server.workers.get(&session) {
                    Some(worker) => worker.run.send(message)?,
                    None => submit(server, &s, message),
                }
                Ok(CommandResult::Done)
            }
            ClientCommand::SessionCompact { session } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                compact_now(server, &s)?;
                Ok(CommandResult::Done)
            }
            ClientCommand::TurnCancel { session } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                if !server.workers.interrupt(&session) {
                    s.cancel();
                }
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
            ClientCommand::ToolList { session } => {
                let context = server.workers.tool_context(server, &session)?;
                let mut tools = server.tools.infos_for(&context.agent);
                // External harnesses ask the user through it.
                if server.workers.get(&session).is_some_and(|h| h.resumable) {
                    tools.push(permission_tool_info());
                }
                Ok(CommandResult::Tools { tools })
            }
            ClientCommand::HarnessList { project } => {
                let folder = resolve_project(&project, &server.config.projects.dir)?;
                Ok(CommandResult::Harnesses {
                    harnesses: server.workers.switches(server, &folder),
                    project: folder,
                })
            }
            ClientCommand::WorkerEvent { .. }
            | ClientCommand::HarnessSet { .. }
            | ClientCommand::TicketChanges { .. }
            | ClientCommand::TicketMerge { .. }
            | ClientCommand::TicketDiscard { .. }
            | ClientCommand::ToolCall { .. }
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

/// A client's attachment references, checked against the session's own uploads.
fn resolve_attachments(
    server: &Server,
    session: &str,
    attachments: &[Attachment],
) -> Result<Vec<Attachment>, String> {
    attachments
        .iter()
        .map(|a| resolve(&server.home, session, a).map(|(a, _)| a))
        .collect()
}
