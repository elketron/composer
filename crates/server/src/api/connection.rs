use base64::Engine;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use composer_protocol::{
    Attachment, ChatMessage, ClientCommand, ClientRequest, CommandResult, DeskBy, DeskTab,
    MessageSource, Role, Seq, ServerMessage, SessionId,
};
use tokio::sync::mpsc;

use crate::api::event_scope::{EventScope, scope};
use crate::api::glob::url_matches;
use crate::api::viewer_proxy;
use crate::app_upstream::AppUpstream;
use crate::attachments::resolve;
use crate::harness::AcpConfig;
use crate::server::Server;
use crate::session::{compact_now, submit};
use crate::tools::ToolContext;

/// One client's WebSocket: what it attached to and subscribed to.
pub struct Connection {
    out: mpsc::UnboundedSender<Arc<ServerMessage>>,
    /// Attached sessions → last seq already sent (replay or live).
    attached: Mutex<HashMap<SessionId, Seq>>,
    subscriptions: Mutex<Vec<String>>,
}

impl Connection {}

impl Connection {
    /// A client's socket.
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

    /// Handles a command. Returns the reply, or `None` when there is none yet: slow commands
    /// (asking the provider, connecting MCP servers) reply later.
    pub fn handle(
        self: &Arc<Self>,
        server: &Arc<Server>,
        request: ClientRequest,
    ) -> Option<ServerMessage> {
        let id = request.id;
        match request.command {
            // Asks the provider: answered from a task.
            ClientCommand::ModelList => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let result =
                        server
                            .provider
                            .models()
                            .await
                            .map(|models| CommandResult::Models {
                                default_model: server.provider.model().to_string(),
                                models,
                            });
                    connection.reply(id, result);
                });
                None
            }
            // MCP servers may need connecting first: answered from a task.
            ClientCommand::ToolList { session } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let result = match ToolContext::for_session(&server, &session) {
                        Ok(context) => Ok(CommandResult::Tools {
                            tools: server
                                .tools
                                .infos_for(&server, context.kind, context.site.as_deref())
                                .await,
                        }),
                        Err(message) => Err(message),
                    };
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::SiteSet { session, site } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let result = set_site(&server, &session, site).await;
                    connection.reply(id, result.map(|text| CommandResult::Note { text }));
                });
                None
            }
            ClientCommand::McpList => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let sites = server.site_mcp.all();
                    let servers = server.mcp().list(&sites).await;
                    connection.reply(id, Ok(CommandResult::McpServers { servers }));
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
                    let reply = match ToolContext::for_session(&server, &session) {
                        Ok(context) => {
                            let result = server.tools.call_value(&context, &name, args).await;
                            ServerMessage::Ok {
                                id,
                                result: CommandResult::ToolOutput {
                                    output: result.text,
                                    is_error: result.is_error,
                                    images: result.images,
                                },
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
            command => Some(match self.run(server, command) {
                Ok(result) => ServerMessage::Ok { id, result },
                Err(message) => ServerMessage::Error {
                    id: Some(id),
                    message,
                },
            }),
        }
    }

    fn run(&self, server: &Arc<Server>, command: ClientCommand) -> Result<CommandResult, String> {
        match command {
            ClientCommand::SessionCreate {
                kind,
                model,
                site,
                harness,
            } => {
                let harness = AcpConfig::check(&server.config(), kind, harness.as_deref())?;
                let model = model
                    .map(|m| m.trim().to_string())
                    .filter(|m| !m.is_empty());
                let site = site
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| crate::agent::Site::load(&server.web, s).map(|site| site.name))
                    .transpose()?;
                let session = server
                    .sessions
                    .create_run(kind, model, site, None, None, harness)
                    .map_err(|e| e.to_string())?;
                // An outside agent starts with its session.
                if session.info().harness.is_some()
                    && let Err(error) = server.acp.start(server, &session)
                {
                    let _ = server.sessions.remove(session.id());
                    return Err(error);
                }
                server.hub.send(ServerMessage::SessionUpdated {
                    session: session.info(),
                });
                // Started in a site: its `folder:` and `mcp.toml` are asked about and its
                // servers started as when the site is set; the reply does not wait for that.
                if let Some(site) = session
                    .info()
                    .site
                    .and_then(|site| crate::agent::Site::load(&server.web, &site).ok())
                {
                    let (server, session) = (server.clone(), session.clone());
                    tokio::spawn(async move {
                        let sink = server.sink(&session);
                        let cancel = tokio_util::sync::CancellationToken::new();
                        let (folder, mcp) =
                            crate::agent::enter_site(&server, &*sink, &site, &cancel).await;
                        if mcp.approved || folder.approved {
                            server.hub.send(ServerMessage::ToolsChanged {
                                session: session.id().to_string(),
                            });
                        }
                    });
                }
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
            ClientCommand::SessionHistory {
                session,
                before,
                messages,
            } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                let (events, more) = s
                    .transcript()
                    .history(before, messages as usize)
                    .map_err(|e| e.to_string())?;
                Ok(CommandResult::History { events, more })
            }
            ClientCommand::SessionDetach { session } => {
                self.attached
                    .lock()
                    .expect("attached lock")
                    .remove(&session);
                Ok(CommandResult::Done)
            }
            ClientCommand::SessionDelete { session } => {
                delete_session(server, &session)?;
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
                submit(server, &s, message);
                Ok(CommandResult::Done)
            }
            ClientCommand::SessionCompact { session } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                compact_now(server, &s)?;
                Ok(CommandResult::Done)
            }
            ClientCommand::TurnCancel { session, seen } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                s.cancel(seen);
                Ok(CommandResult::Done)
            }
            ClientCommand::ModelSet { session, model } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                let model = model
                    .map(|m| m.trim().to_string())
                    .filter(|m| !m.is_empty());
                s.update_info(&server.hub, |info| info.model = model);
                Ok(CommandResult::Session { session: s.info() })
            }
            ClientCommand::ApprovalRespond { request, decision } => {
                if server.pending_approvals.resolve(&request, decision) {
                    Ok(CommandResult::Done)
                } else {
                    Err(format!("approval request `{request}` is no longer open"))
                }
            }
            ClientCommand::ConfigReload => {
                let reloaded = server.reload_config().map_err(|e| e.to_string())?;
                if let Some(restart_needed) = &reloaded {
                    // The other clients learn it as the watcher would have told them.
                    server.hub.send(ServerMessage::ConfigReloaded {
                        restart_needed: restart_needed.clone(),
                        error: None,
                    });
                }
                Ok(CommandResult::ConfigReloaded {
                    changed: reloaded.is_some(),
                    restart_needed: reloaded.unwrap_or_default(),
                })
            }
            ClientCommand::ModelList
            | ClientCommand::ToolCall { .. }
            | ClientCommand::ToolList { .. }
            | ClientCommand::McpList
            | ClientCommand::SiteSet { .. } => Err("handled asynchronously".into()),
            ClientCommand::Subscribe { urls } => {
                *self.subscriptions.lock().expect("subs lock") = urls;
                Ok(CommandResult::Done)
            }
            ClientCommand::TerminalOpen {
                session,
                terminal,
                cols,
                rows,
                command,
            } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                let (id, scrollback) = server.terminals.open(
                    server,
                    &s,
                    terminal.as_deref(),
                    command.as_deref(),
                    cols,
                    rows,
                )?;
                Ok(CommandResult::Terminal {
                    terminal: id,
                    scrollback: base64::engine::general_purpose::STANDARD.encode(scrollback),
                })
            }
            ClientCommand::TerminalInput { terminal, data } => {
                let found = server.terminals.find(&terminal)?;
                found.input(data.as_bytes())?;
                Ok(CommandResult::Done)
            }
            ClientCommand::TerminalResize {
                terminal,
                cols,
                rows,
            } => {
                let found = server.terminals.find(&terminal)?;
                found.resize(cols, rows)?;
                Ok(CommandResult::Done)
            }
            ClientCommand::TerminalRead { terminal } => {
                let found = server.terminals.find(&terminal)?;
                Ok(CommandResult::Terminal {
                    terminal,
                    scrollback: base64::engine::general_purpose::STANDARD
                        .encode(found.scrollback()),
                })
            }
            ClientCommand::TerminalClose { terminal } => {
                server.terminals.close(server, &terminal)?;
                Ok(CommandResult::Done)
            }
            ClientCommand::ProcessList { session } => {
                server.sessions.get(&session).map_err(|e| e.to_string())?;
                Ok(CommandResult::Processes {
                    processes: server.terminals.processes(&session),
                })
            }
            ClientCommand::ProcessStop { terminal } => {
                server
                    .terminals
                    .stop_process(&terminal, crate::terminals::StoppedBy::User)?;
                Ok(CommandResult::Done)
            }
            ClientCommand::DeskGet { session } => Ok(CommandResult::Desk {
                desk: server
                    .sessions
                    .get(&session)
                    .map_err(|e| e.to_string())?
                    .desk(),
            }),
            ClientCommand::DeskOpen {
                session,
                tab,
                focus,
            } => {
                if tab == DeskTab::LiveView && viewer_proxy::viewer_path(server).is_none() {
                    return Err(viewer_proxy::NO_LIVE_VIEW.into());
                }
                if let DeskTab::App { name, .. } = &tab
                    && AppUpstream::find(&server.config(), name).is_none()
                {
                    return Err(crate::api::app_proxy::no_app(name));
                }
                change_desk(server, &session, |desk| {
                    desk.open(tab, focus, DeskBy::User).map(|_| ())
                })
            }
            ClientCommand::DeskClose { session, tab } => {
                change_desk(server, &session, |desk| desk.close(&tab))
            }
            ClientCommand::DeskFocus { session, tab } => {
                change_desk(server, &session, |desk| desk.focus(&tab))
            }
            ClientCommand::DeskMove { session, tab, to } => {
                change_desk(server, &session, |desk| desk.move_to(&tab, to))
            }
        }
    }
}

/// `session.delete`: its turn stops, its outside agent, terminals, background processes and
/// watches end, its folder goes, and every client hears it.
fn delete_session(server: &Arc<Server>, id: &str) -> Result<(), String> {
    let session = server.sessions.get(id).map_err(|e| e.to_string())?;
    session.cancel(None);
    server.acp.stop(id);
    // Ended by the server, not the user: nobody is told in a session that is gone.
    let processes = server.terminals.processes(id);
    for process in &processes {
        let _ = server
            .terminals
            .stop_process(&process.terminal, crate::terminals::StoppedBy::Server);
    }
    for tab in session.desk().tabs {
        if let DeskTab::Terminal { id: terminal } = tab.tab
            && !processes.iter().any(|p| p.terminal == terminal)
        {
            let _ = server.terminals.close(server, &terminal);
        }
    }
    server.watches.drop_session(id);
    server.sessions.remove(id).map_err(|e| e.to_string())?;
    server.hub.send(ServerMessage::SessionDeleted {
        session: id.to_string(),
    });
    Ok(())
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

/// `site.set`: the user puts a session in site `site`, or (`None`, or blank) out of its site.
/// The text is what happened; the session's new state goes out as `session.updated`.
async fn set_site(
    server: &Arc<Server>,
    session: &str,
    site: Option<String>,
) -> Result<String, String> {
    let session = server.sessions.get(session).map_err(|e| e.to_string())?;
    // The user's own switch: an approval it asks for waits as long as they take.
    let cancel = tokio_util::sync::CancellationToken::new();
    crate::agent::set_site(server, &*server.sink(&session), site.as_deref(), &cancel)
        .await
        .map(|set| set.text())
}

/// Changes `session`'s desk (the user's change) and tells its clients: the desk after.
fn change_desk(
    server: &Server,
    session: &str,
    change: impl FnOnce(&mut crate::desk::SessionDesk) -> Result<(), String>,
) -> Result<CommandResult, String> {
    let session = server.sessions.get(session).map_err(|e| e.to_string())?;
    let desk = session.change_desk(change)?;
    server.hub.send(ServerMessage::DeskChanged {
        session: session.id().to_string(),
        desk: desk.clone(),
        by: DeskBy::User,
        opened: None,
    });
    Ok(CommandResult::Desk { desk })
}
