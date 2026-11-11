use base64::Engine;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use composer_protocol::{
    Attachment, ChangedBy, ChatMessage, ClientCommand, ClientRequest, CommandResult, DeskBy,
    DeskTab, MessageSource, Role, Seq, ServerMessage, SessionId,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::actions::Caller;
use crate::agent::AgentDefinition;
use crate::api::app_commands::app_may;
use crate::api::event_scope::{EventScope, scope};
use crate::api::glob::url_matches;
use crate::api::{Credential, viewer_proxy};
use crate::apps::{AppGrant, ProjectName};
use crate::attachments::resolve;
use crate::harness::{PERMISSION_TOOL, WorkerSink, answer_permission, permission_tool_info};
use crate::server::Server;
use crate::session::{compact_now, submit};
use crate::slot_guard::SlotGuard;
use crate::worker::ticket::TicketId;

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
    /// An app's socket: only the commands its token allows.
    app: Option<AppGrant>,
    /// What the socket was authenticated with (`None`: an app's token, which is no workspace
    /// credential): who a one-time code it asks for stands for.
    credential: Option<Credential>,
    /// The projects this client opened (`project.open`); closed when it goes.
    opened: Mutex<HashSet<ProjectName>>,
    /// A desktop window's connection (ticket 0177): its id and its TUI's terminal; the TUI ends
    /// when the window goes.
    window: Mutex<Option<(String, String)>>,
    /// The window this connection is the TUI of (`window.join`): its events come here too.
    joined: Mutex<Option<String>>,
}

impl Connection {
    /// A workspace client's socket, authenticated with `credential`.
    pub fn new(out: mpsc::UnboundedSender<Arc<ServerMessage>>, credential: Credential) -> Self {
        Self {
            credential: Some(credential),
            ..Self::unauthenticated(out)
        }
    }

    fn unauthenticated(out: mpsc::UnboundedSender<Arc<ServerMessage>>) -> Self {
        Self {
            out,
            attached: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(Vec::new()),
            worker_session: Mutex::new(None),
            grants: Mutex::new(HashMap::new()),
            next_grant: AtomicU64::new(0),
            app: None,
            credential: None,
            opened: Mutex::new(HashSet::new()),
            window: Mutex::new(None),
            joined: Mutex::new(None),
        }
    }

    /// The socket of an app opened on a project (ticket 0163): it may subscribe to pages, list
    /// pipelines and apps, and start and run its own site's works; nothing else.
    pub fn for_app(out: mpsc::UnboundedSender<Arc<ServerMessage>>, grant: AppGrant) -> Self {
        Self {
            app: Some(grant),
            ..Self::unauthenticated(out)
        }
    }

    /// Opens `project` (a name or a folder) for this client, once.
    async fn open_project(
        self: &Arc<Self>,
        server: &Arc<Server>,
        project: &str,
    ) -> Result<CommandResult, String> {
        let (server2, project) = (server.clone(), project.to_string());
        let (name, folder) =
            tokio::task::spawn_blocking(move || crate::apps::find_project_by(&server2, &project))
                .await
                .map_err(|e| e.to_string())??;
        Ok(CommandResult::ProjectOpened {
            project: name.to_string(),
            first: self.open(server, &name, &folder),
        })
    }

    /// `app.allow` (`allow`) or `app.promote` of a project-local app; replies later.
    fn local_app(
        self: &Arc<Self>,
        server: &Arc<Server>,
        id: u64,
        project: String,
        app: String,
        allow: bool,
    ) {
        let (connection, server) = (self.clone(), server.clone());
        tokio::spawn(async move {
            let done = tokio::task::spawn_blocking(move || match allow {
                true => crate::apps::allow_local(&server, &project, &app),
                false => crate::apps::promote_local(&server, &project, &app),
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
            connection.reply(id, done.map(|text| CommandResult::Note { text }));
        });
    }

    /// This client has `name` open (`true`: the first in the workspace); again does nothing.
    pub fn open(&self, server: &Arc<Server>, name: &ProjectName, folder: &std::path::Path) -> bool {
        if !self
            .opened
            .lock()
            .expect("opened lock")
            .insert(name.clone())
        {
            return false;
        }
        server.open_projects.open(server, name, folder)
    }

    /// A window's TUI (unsandboxed: its editor can run a shell) takes input only from its own
    /// window's connection (ticket 0178); a session's terminal from any client of the workspace.
    fn may_drive(&self, terminal: &crate::terminals::Terminal) -> Result<(), String> {
        let Some(window) = &terminal.window else {
            return Ok(());
        };
        let own = self.window.lock().expect("window lock");
        match own.as_ref().is_some_and(|(id, _)| id == window) {
            true => Ok(()),
            false => Err(format!(
                "terminal `{}` is window {window}'s TUI: only that window types into it",
                terminal.id
            )),
        }
    }

    /// The client is gone: what it opened, it closes (a window's TUI too).
    pub fn close_all(&self, server: &Server) {
        for name in self.opened.lock().expect("opened lock").drain() {
            server.open_projects.close(&name);
        }
        if let Some((_, terminal)) = self.window.lock().expect("window lock").take() {
            let _ = server.terminals.close(server, &terminal);
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
            EventScope::Window(window) => {
                self.window
                    .lock()
                    .expect("window lock")
                    .as_ref()
                    .is_some_and(|(id, _)| id == window)
                    // The window's TUI gets the window's session changes only: its own
                    // terminal's output would make it redraw for ever.
                    || (matches!(message, ServerMessage::WindowChanged { .. })
                        && self.joined.lock().expect("joined lock").as_deref() == Some(window))
            }
            EventScope::Project(name) => {
                self.app
                    .as_ref()
                    .and_then(|g| g.project.as_ref())
                    .is_some_and(|p| p.name.as_str() == name)
                    || self
                        .opened
                        .lock()
                        .expect("opened lock")
                        .iter()
                        .any(|p| p.as_str() == name)
            }
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
        let mut request = request;
        if let Some(grant) = &self.app
            && let Err(message) = app_may(grant, &mut request.command)
        {
            return Some(ServerMessage::Error {
                id: Some(id),
                message,
            });
        }
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
                    let result = match server.use_project(&project) {
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
                    let result = match TicketId::parse(&ticket) {
                        Ok(ticket) => crate::worker::ticket_changes(&server, &ticket, None)
                            .await
                            .map(|changes| CommandResult::Changes { changes }),
                        Err(e) => Err(e),
                    };
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::WorkRun {
                work,
                stage,
                session,
            } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let result = run_work(&server, &work, stage.as_deref(), session).await;
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::TicketMerge { ticket } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::task::spawn_blocking(move || {
                    let result = TicketId::parse(&ticket)
                        .and_then(|ticket| crate::worker::merge_ticket(&server, &ticket, None))
                        .map(|commit| CommandResult::Merged { commit });
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::TicketComment { ticket, comments } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::task::spawn_blocking(move || {
                    let result = TicketId::parse(&ticket)
                        .and_then(|ticket| {
                            crate::worker::ticket_comments(&server, &ticket, &comments)
                        })
                        .map(|lines| CommandResult::Comments { lines });
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::TicketDiscard { ticket } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::task::spawn_blocking(move || {
                    let result = TicketId::parse(&ticket)
                        .and_then(|ticket| crate::worker::discard_ticket(&server, &ticket))
                        .map(|()| CommandResult::Done);
                    connection.reply(id, result);
                });
                None
            }
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
                    let result = match server.workers.tool_context(&server, &session) {
                        Ok(context) => {
                            let mut tools = server
                                .tools
                                .infos_for(&server.mcp(), &context.agent, context.site.as_deref())
                                .await;
                            // External harnesses ask the user through it.
                            if server.workers.get(&session).is_some_and(|h| h.external) {
                                tools.push(permission_tool_info());
                            }
                            Ok(CommandResult::Tools { tools })
                        }
                        Err(message) => Err(message),
                    };
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::LibraryList => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::task::spawn_blocking(move || {
                    connection.reply(id, crate::api::library_commands::list(&server));
                });
                None
            }
            ClientCommand::LibraryInstall { site, name } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let result =
                        crate::api::library_commands::install(&server, &site, name.as_deref())
                            .await;
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::LibraryUpdate { dry_run } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let result = crate::api::library_commands::update(&server, dry_run).await;
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::LibraryPublish {
                site,
                name,
                approved,
            } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::task::spawn_blocking(move || {
                    let result = crate::api::library_commands::publish(
                        &server,
                        &site,
                        name.as_deref(),
                        approved.as_deref(),
                    );
                    connection.reply(id, result);
                });
                None
            }
            ClientCommand::ProjectOpen { project } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let reply = connection.open_project(&server, &project).await;
                    connection.reply(id, reply);
                });
                None
            }
            ClientCommand::ProjectClose { project } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let found = {
                        let server = server.clone();
                        tokio::task::spawn_blocking(move || {
                            crate::apps::find_project_by(&server, &project)
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r)
                    };
                    let reply = found.map(|(name, _)| {
                        if connection.opened.lock().expect("opened lock").remove(&name) {
                            server.open_projects.close(&name);
                        }
                        CommandResult::Done
                    });
                    connection.reply(id, reply);
                });
                None
            }
            ClientCommand::AppOpen {
                site,
                project,
                session,
            } => {
                // An app's socket never gets here (`app_may`); without a workspace credential
                // nothing is opened.
                let Some(minter) = self.credential.clone() else {
                    return Some(ServerMessage::Error {
                        id: Some(id),
                        message: "only a client of the workspace opens apps".into(),
                    });
                };
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let opened = tokio::task::spawn_blocking(move || {
                        crate::apps::open_app(
                            &server,
                            &site,
                            project.as_deref(),
                            session.as_deref(),
                            &minter,
                        )
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r);
                    connection.reply(id, opened.map(|opened| CommandResult::AppOpened { opened }));
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
            ClientCommand::AppsRebuild { all, yes } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let text = rebuild_note(&server, all, yes).await;
                    connection.reply(id, Ok(CommandResult::Note { text }));
                });
                None
            }
            ClientCommand::AppAllow { project, app } => {
                self.local_app(server, id, project, app, true);
                None
            }
            ClientCommand::AppPromote { project, app } => {
                self.local_app(server, id, project, app, false);
                None
            }
            ClientCommand::ProjectsOpen => Some(ServerMessage::Ok {
                id,
                result: CommandResult::OpenProjects {
                    projects: server.open_projects.list(),
                },
            }),
            ClientCommand::AppsList { project } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let listed = tokio::task::spawn_blocking(move || match project {
                        Some(name) => crate::apps::project_apps(&server, &name),
                        None => Ok(crate::apps::home_apps(&server)),
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r);
                    connection.reply(id, listed.map(|list| CommandResult::Apps { list }));
                });
                None
            }
            ClientCommand::McpList => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let agents = {
                        let index = server.index.read().expect("index lock");
                        AgentDefinition::all(&server.web, &index)
                    };
                    let servers = server.mcp().list(&agents).await;
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
                    if name == PERMISSION_TOOL
                        && server.workers.get(&session).is_some_and(|h| h.external)
                    {
                        let output = answer_permission(&server, &session, args).await;
                        connection.reply(
                            id,
                            Ok(CommandResult::ToolOutput {
                                output,
                                is_error: false,
                                images: vec![],
                            }),
                        );
                        return;
                    }
                    let reply = match server.workers.tool_context(&server, &session) {
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
            ClientCommand::WorkerStuck { summary } => {
                let (connection, server) = (self.clone(), server.clone());
                tokio::spawn(async move {
                    let worker = connection
                        .worker_session
                        .lock()
                        .expect("worker lock")
                        .clone();
                    let result = match worker {
                        Some(worker) => server.workers.stuck(&server, &worker, summary).await.map(
                            |(message, compact)| CommandResult::WorkerGoOn { message, compact },
                        ),
                        None => Err("not a worker connection".into()),
                    };
                    connection.reply(id, result);
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
                        Ok((approved, reason)) => ServerMessage::Ok {
                            id,
                            result: CommandResult::Approval { approved, reason },
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

    /// A worker's recipe approval: already approved here, or asked of this server's clients (or
    /// decided by its orchestrator: for this run only, with its reason).
    async fn ask_approval(
        &self,
        server: &Arc<Server>,
        site: &str,
        recipe: &str,
        args: &[String],
        body: String,
        hash: &str,
    ) -> Result<(bool, Option<String>), String> {
        if server
            .approvals
            .is_approved(site, recipe, hash, Caller::Agent)
        {
            return Ok((true, None));
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
                Caller::Agent,
                &CancellationToken::new(),
            )
            .await;
        let approved = decision.as_ref().is_some_and(|answer| {
            matches!(
                answer.decision,
                composer_protocol::ApprovalDecision::Approve
                    | composer_protocol::ApprovalDecision::Always
            )
        });
        let reason = decision.and_then(|answer| answer.reason);
        if approved && reason.is_none() {
            server
                .approvals
                .approve(site, recipe, hash, Caller::Agent)
                .map_err(|e| e.to_string())?;
        }
        Ok((approved, reason))
    }

    fn run(&self, server: &Arc<Server>, command: ClientCommand) -> Result<CommandResult, String> {
        match command {
            ClientCommand::SessionCreate { agent, project } => {
                AgentDefinition::load(&server.web, &agent).map_err(|e| e.to_string())?;
                let project = project
                    .as_deref()
                    .map(|p| server.use_project(p))
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
                // A paused worker (a limit, a restart) is stopped for good.
                if !server.workers.interrupt(&session)
                    && !server.workers.stop_paused(server, &session)
                {
                    s.cancel();
                }
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
            ClientCommand::ProjectSet { session, path } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                let project = path
                    .as_deref()
                    .filter(|p| !p.trim().is_empty())
                    .map(|p| server.use_project(p))
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
            ClientCommand::ProjectOverview { project } => {
                crate::project::project_overview(server, &project)
                    .map(|overview| CommandResult::Project { overview })
            }
            ClientCommand::PipelineList => Ok(CommandResult::Pipelines {
                pipelines: crate::pipeline::pipelines(server)
                    .iter()
                    .map(|p| p.summary())
                    .collect(),
                templates: crate::pipeline::templates(server)
                    .iter()
                    .map(|t| t.summary())
                    .collect(),
                works: crate::pipeline::pipelines(server)
                    .into_iter()
                    .flat_map(|p| {
                        p.works()
                            .into_iter()
                            .filter_map(move |w| crate::pipeline::Work::load(p.clone(), &w))
                    })
                    .map(|w| w.summary())
                    .collect(),
            }),
            ClientCommand::PipelineNew { kind, name } => {
                crate::pipeline::new_pipeline(server, &kind, name.as_deref(), ChangedBy::App).map(
                    |p| {
                        crate::pipeline::prepare_board(server, &p.name);
                        CommandResult::Pipeline {
                            pipeline: p.summary(),
                        }
                    },
                )
            }
            ClientCommand::WorkNew {
                pipeline,
                name,
                project,
                brief,
            } => crate::pipeline::new_work(
                server,
                crate::pipeline::NewWork {
                    pipeline: &pipeline,
                    name: &name,
                    title: None,
                    project: project.as_deref(),
                    brief: brief.as_deref(),
                },
                ChangedBy::App,
            )
            .map(|w| CommandResult::Work { work: w.summary() }),
            ClientCommand::HarnessList { project } => {
                let folder = server.see_project(&project)?;
                Ok(CommandResult::Harnesses {
                    harnesses: server.workers.switches(server, &folder),
                    project: folder,
                })
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
            ClientCommand::WorkerEvent { .. }
            | ClientCommand::HarnessSet { .. }
            | ClientCommand::TicketChanges { .. }
            | ClientCommand::TicketMerge { .. }
            | ClientCommand::TicketComment { .. }
            | ClientCommand::TicketDiscard { .. }
            | ClientCommand::ModelList
            | ClientCommand::WorkRun { .. }
            | ClientCommand::ToolCall { .. }
            | ClientCommand::ToolList { .. }
            | ClientCommand::McpList
            | ClientCommand::AppsList { .. }
            | ClientCommand::ProjectOpen { .. }
            | ClientCommand::ProjectClose { .. }
            | ClientCommand::ProjectsOpen
            | ClientCommand::AppOpen { .. }
            | ClientCommand::SiteSet { .. }
            | ClientCommand::AppsRebuild { .. }
            | ClientCommand::AppAllow { .. }
            | ClientCommand::AppPromote { .. }
            | ClientCommand::LibraryList
            | ClientCommand::LibraryInstall { .. }
            | ClientCommand::LibraryUpdate { .. }
            | ClientCommand::LibraryPublish { .. }
            | ClientCommand::SlotAcquire
            | ClientCommand::SlotRelease { .. }
            | ClientCommand::WorkerStuck { .. }
            | ClientCommand::ApprovalAsk { .. } => Err("handled asynchronously".into()),
            ClientCommand::Subscribe { urls } => {
                *self.subscriptions.lock().expect("subs lock") = urls;
                Ok(CommandResult::Done)
            }
            ClientCommand::WindowJoin { window } => {
                // Only a window that exists: its TUI's terminal is running.
                if !server.terminals.has_window(&window) {
                    return Err(format!("no window `{window}`"));
                }
                *self.joined.lock().expect("joined lock") = Some(window);
                Ok(CommandResult::Done)
            }
            ClientCommand::WindowHello { cols, rows } => {
                let mut window = self.window.lock().expect("window lock");
                if let Some((id, terminal)) = window.as_ref() {
                    return Ok(CommandResult::Window {
                        window: id.clone(),
                        terminal: terminal.clone(),
                    });
                }
                let id = format!("w{:08x}", rand::random::<u32>());
                let terminal = server.terminals.open_window(server, &id, cols, rows)?;
                *window = Some((id.clone(), terminal.clone()));
                Ok(CommandResult::Window {
                    window: id,
                    terminal,
                })
            }
            ClientCommand::WindowSession { window, session } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                server.hub.send(ServerMessage::WindowChanged {
                    window,
                    session: s.id().to_string(),
                });
                Ok(CommandResult::Done)
            }
            ClientCommand::TerminalOpen {
                session,
                terminal,
                cols,
                rows,
            } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                let (id, scrollback) =
                    server
                        .terminals
                        .open(server, &s, terminal.as_deref(), cols, rows)?;
                Ok(CommandResult::Terminal {
                    terminal: id,
                    scrollback: base64::engine::general_purpose::STANDARD.encode(scrollback),
                })
            }
            ClientCommand::TerminalInput { terminal, data } => {
                let found = server.terminals.find(&terminal)?;
                self.may_drive(&found)?;
                found.input(data.as_bytes())?;
                Ok(CommandResult::Done)
            }
            ClientCommand::TerminalResize {
                terminal,
                cols,
                rows,
            } => {
                let found = server.terminals.find(&terminal)?;
                self.may_drive(&found)?;
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
            ClientCommand::SelectionClear { session } => {
                let s = server.sessions.get(&session).map_err(|e| e.to_string())?;
                if s.set_selection(None) {
                    server.hub.send(ServerMessage::SelectionChanged {
                        session,
                        selection: None,
                    });
                }
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

/// `work.run`: the stage's worker belongs to `session`, else to a new session in the work's
/// project.
async fn run_work(
    server: &Arc<Server>,
    work: &str,
    stage: Option<&str>,
    session: Option<String>,
) -> Result<CommandResult, String> {
    let parent = match session {
        Some(id) => server.sessions.get(&id).map_err(|e| e.to_string())?,
        None => {
            let found = crate::pipeline::find_work(server.web.root(), work)?;
            crate::pipeline::work_session(server, &found, None)?
        }
    };
    let started = crate::pipeline::run_stage(server, &parent, work, stage, None).await?;
    Ok(CommandResult::Work {
        work: started.work.summary(),
    })
}

/// `site.set`: the user enters site `site` for a session, or (`None`, or blank) leaves its site.
/// The text is what happened, the site's application included; the session's new state goes out
/// as `session.updated`.
async fn set_site(
    server: &Arc<Server>,
    session: &str,
    site: Option<String>,
) -> Result<String, String> {
    let session = server.sessions.get(session).map_err(|e| e.to_string())?;
    let name = site.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let Some(name) = name else {
        return Ok(match crate::agent::leave_site(server, &session) {
            Some(previous) => format!("Left site {previous}."),
            None => "The session was not in a site.".into(),
        });
    };
    let entered = crate::agent::enter_site_as_client(server, &session, name).await?;
    let mut text = format!("Entered {} ({}).", entered.site, entered.title);
    let notes = entered.notes();
    if !notes.is_empty() {
        text.push(' ');
        text.push_str(&notes);
    }
    if entered.live_view {
        text.push_str(" The live view is on the stage.");
    }
    Ok(text)
}

/// `apps.rebuild`: what would run, or (with `yes`) what ran.
async fn rebuild_note(server: &Arc<Server>, all: bool, yes: bool) -> String {
    if !yes {
        let plan = crate::apps::rebuild_plan(server, all).await;
        if plan.is_empty() {
            return "every app is built with the current sdk: nothing to rebuild".into();
        }
        let shown: Vec<String> = plan
            .iter()
            .map(|(site, recipe)| format!("site://{site}:\n{recipe}"))
            .collect();
        return format!(
            "{}\n\nNothing was run. Run again with --yes to build these {} app(s).",
            shown.join("\n"),
            plan.len()
        );
    }
    let done = crate::apps::rebuild(server, all).await;
    if done.is_empty() {
        return "every app is built with the current sdk: nothing to rebuild".into();
    }
    done.iter()
        .map(|(site, status, output)| match status.as_str() {
            "built" => format!("{site}: built"),
            _ => format!("{site}: {status}\n{output}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
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
