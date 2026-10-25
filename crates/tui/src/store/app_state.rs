use composer_protocol::{ApprovalKind, ClientCommand, CommandResult, ServerMessage};

use crate::action::Action;
use crate::effect::Effect;
use crate::store::types::{
    Catalog, ConnState, FileSource, FlashLevel, Mode, Outgoing, Pane, Pending, PendingApproval,
    PickerKind, PickerTarget, Startup,
};
use crate::store::{
    ApprovalStore, AttachmentStore, ConnectionStore, PageStore, PickerStore, ProjectStore,
    ProjectViewStore, SessionStore, UiStore,
};

/// All shared state. `reduce` is the only way it changes; it does no IO.
#[derive(Debug)]
pub struct AppState {
    pub conn: ConnectionStore,
    pub session: SessionStore,
    pub page: PageStore,
    pub ui: UiStore,
    pub picker: PickerStore,
    pub approvals: ApprovalStore,
    pub project: ProjectStore,
    /// The project view (`gp`).
    pub project_view: ProjectViewStore,
    pub catalog: Catalog,
    /// Files attached to the message being written.
    pub attachments: AttachmentStore,
    startup: Option<Startup>,
    default_agent: String,
    /// Where project names point (`:project composer`), when known.
    pub projects_dir: Option<String>,
}

impl AppState {
    pub fn new(startup: Startup, context_limit: u32) -> Self {
        Self {
            conn: ConnectionStore::new(context_limit),
            session: SessionStore::default(),
            page: PageStore::new(),
            ui: UiStore::new(),
            picker: PickerStore::new(),
            approvals: ApprovalStore::default(),
            project: ProjectStore::default(),
            project_view: ProjectViewStore::default(),
            catalog: Catalog::default(),
            attachments: AttachmentStore::default(),
            default_agent: startup.agent.clone(),
            projects_dir: startup.projects_dir.clone(),
            startup: Some(startup),
        }
    }

    pub fn reduce(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Quit => {
                self.ui.quit = true;
                vec![Effect::Quit]
            }
            Action::SetMode(mode) => {
                self.ui.set_mode(mode);
                // Fresh sites and pages to complete from.
                if mode == Mode::Command {
                    vec![Effect::LoadCatalog]
                } else {
                    vec![]
                }
            }
            Action::FocusPane(pane) => {
                self.ui.focus = pane;
                if self.ui.maximized.is_some() {
                    self.ui.maximized = Some(pane);
                }
                vec![]
            }
            Action::FocusNext => {
                self.ui.focus_next();
                vec![]
            }
            Action::ToggleMaximize => {
                self.ui.toggle_maximize();
                vec![]
            }
            Action::ToggleThinking => {
                self.ui.show_thinking = !self.ui.show_thinking;
                vec![]
            }
            Action::ToggleHelp => {
                self.ui.help_open = !self.ui.help_open;
                vec![]
            }
            Action::ToggleToolDetails => {
                self.ui.show_tool_details = !self.ui.show_tool_details;
                vec![]
            }
            Action::Flash(text, level) => {
                self.ui.flash(text, level);
                vec![]
            }

            Action::SendMessage(text) => {
                let Some(session) = self.session.current.clone() else {
                    self.ui.flash("no session yet", FlashLevel::Warn);
                    return vec![];
                };
                if let Some(why) = self.attachments.blocking() {
                    self.ui.flash(format!("not sent: {why}"), FlashLevel::Warn);
                    return vec![];
                }
                self.ui.flash = None;
                let attachments = self.attachments.take_ready(&session);
                self.send(
                    ClientCommand::MessageSend {
                        session,
                        text,
                        pages: vec![],
                        images: vec![],
                        attachments,
                    },
                    Pending::Ignore,
                )
            }
            Action::AttachPath(path) => {
                let Some(session) = self.session.current.clone() else {
                    self.ui.flash("no session yet", FlashLevel::Warn);
                    return vec![];
                };
                let name = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.clone());
                let id = self.attachments.add(name, session.clone());
                vec![Effect::UploadFile {
                    id,
                    session,
                    source: FileSource::Path(path),
                }]
            }
            Action::AttachClipboard => {
                if self.session.current.is_none() {
                    self.ui.flash("no session yet", FlashLevel::Warn);
                    return vec![];
                }
                vec![Effect::ReadClipboard]
            }
            Action::ClipboardRead(read) => {
                let (Ok((name, bytes)), Some(session)) =
                    (read.clone(), self.session.current.clone())
                else {
                    if let Err(e) = read {
                        self.ui.flash(e, FlashLevel::Warn);
                    }
                    return vec![];
                };
                let id = self.attachments.add(name.clone(), session.clone());
                vec![Effect::UploadFile {
                    id,
                    session,
                    source: FileSource::Bytes { name, bytes },
                }]
            }
            Action::FileUploaded { id, result } => {
                if let Err(e) = &result {
                    self.ui
                        .flash(format!("upload failed: {e}"), FlashLevel::Error);
                }
                self.attachments.uploaded(id, result);
                vec![]
            }
            Action::DropAttachment => {
                if let Some(file) = self.attachments.remove_last() {
                    self.ui
                        .flash(format!("removed {}", file.name), FlashLevel::Info);
                }
                vec![]
            }
            Action::ClearAttachments => {
                self.attachments.clear();
                vec![]
            }
            Action::CancelTurn => match self.session.current.clone() {
                Some(session) if self.session.is_busy() => {
                    self.send(ClientCommand::TurnCancel { session }, Pending::Ignore)
                }
                _ => vec![],
            },
            Action::Compact => match self.session.current.clone() {
                Some(session) => {
                    self.ui.flash(
                        "compacting: saving memory, then summarizing",
                        FlashLevel::Info,
                    );
                    self.send(ClientCommand::SessionCompact { session }, Pending::Ignore)
                }
                None => vec![],
            },
            Action::NewSession { agent } => {
                let agent = agent.unwrap_or_else(|| self.default_agent.clone());
                // A worker's project is its worktree: the new chat gets the orchestrator's.
                let project = self
                    .session
                    .current_info()
                    .and_then(|s| self.session.project_of(s));
                self.send(
                    ClientCommand::SessionCreate { agent, project },
                    Pending::CreateSession,
                )
            }
            Action::AttachSession(id) => self.attach(&id, true),
            Action::JumpBack => match self.session.jumplist.pop() {
                Some(id) => self.attach(&id, false),
                None => {
                    self.ui.flash("nothing to jump back to", FlashLevel::Info);
                    vec![]
                }
            },
            Action::CycleOrchestration(delta) => {
                let members = self.session.orchestration();
                let Some(here) = members
                    .iter()
                    .position(|s| Some(&s.id) == self.session.current.as_ref())
                else {
                    self.no_orchestration();
                    return vec![];
                };
                let next = (here as i32 + delta).rem_euclid(members.len() as i32) as usize;
                let id = members[next].id.clone();
                self.attach(&id, true)
            }
            Action::SetProject(path) => match self.session.current.clone() {
                Some(session) => {
                    if path.is_none() {
                        self.ui.flash(
                            "project unset (new sessions from here start without one)",
                            FlashLevel::Info,
                        );
                    }
                    self.send(ClientCommand::ProjectSet { session, path }, Pending::Ignore)
                }
                None => vec![],
            },

            Action::Harness { on, entry } => {
                let project = self
                    .session
                    .current
                    .as_deref()
                    .and_then(|id| self.session.sessions.iter().find(|s| s.id == id))
                    .and_then(|s| s.project.clone());
                let Some(project) = project else {
                    self.ui.flash(
                        "no project here (:project <name> sets one)",
                        FlashLevel::Warn,
                    );
                    return vec![];
                };
                let command = match on {
                    Some(on) => ClientCommand::HarnessSet { project, entry, on },
                    None => ClientCommand::HarnessList { project },
                };
                self.send(command, Pending::Harnesses)
            }

            Action::OpenUrl(url) => {
                if !url.starts_with("site://") {
                    self.ui
                        .flash(format!("external link: {url}"), FlashLevel::Info);
                    return vec![];
                }
                // You navigated yourself: stop following the agent and look at the page.
                self.page.follow = false;
                self.reduce(Action::FocusPane(Pane::Page));
                vec![self.page.navigate(url)]
            }
            Action::PageBack => self.page.back().into_iter().collect(),
            Action::PageForward => self.page.forward().into_iter().collect(),
            Action::ReloadPage if self.project.open => self.refresh_changes(),
            Action::ReloadPage => self.page.reload().into_iter().collect(),
            Action::ToggleChanges => {
                self.project.open = !self.project.open;
                if !self.project.open {
                    return vec![];
                }
                // One view in the right pane at a time.
                self.project_view.open = false;
                self.reduce(Action::FocusPane(Pane::Page));
                self.refresh_changes()
            }
            Action::ToggleProjectView if self.project_view.open => {
                self.project_view.open = false;
                vec![]
            }
            Action::ToggleProjectView => self.reduce(Action::ProjectView(None)),
            Action::ProjectView(project) => {
                let Some(project) = project.or_else(|| {
                    self.session
                        .current_info()
                        .and_then(|s| self.session.project_of(s))
                }) else {
                    self.ui
                        .flash("no project here: :project view <name>", FlashLevel::Warn);
                    return vec![];
                };
                self.project.open = false;
                self.project_view.open = true;
                self.project_view.loading = true;
                if self.project_view.project.as_deref() != Some(project.as_str()) {
                    self.project_view.overview = None;
                }
                self.project_view.project = Some(project.clone());
                self.reduce(Action::FocusPane(Pane::Page));
                self.send(
                    ClientCommand::ProjectOverview { project },
                    Pending::Overview,
                )
            }
            Action::McpServers => {
                self.ui.flash("connecting the MCP servers…", FlashLevel::Info);
                self.send(ClientCommand::McpList, Pending::McpServers)
            }
            Action::ReloadConfig => self.send(ClientCommand::ConfigReload, Pending::ReloadConfig),
            Action::Pipelines => self.send(ClientCommand::PipelineList, Pending::Pipelines),
            Action::PipelineNew { kind, name } => {
                self.send(ClientCommand::PipelineNew { kind, name }, Pending::Pipeline)
            }
            Action::WorkNew {
                pipeline,
                name,
                project,
            } => {
                let project = project
                    .or_else(|| {
                        self.project_view
                            .open
                            .then(|| {
                                self.project_view
                                    .overview
                                    .as_ref()
                                    .map(|o| o.folder.clone())
                            })
                            .flatten()
                    })
                    .or_else(|| {
                        self.session
                            .current_info()
                            .and_then(|s| self.session.project_of(s))
                    });
                self.send(
                    ClientCommand::WorkNew {
                        pipeline,
                        name,
                        project,
                        brief: None,
                    },
                    Pending::Work,
                )
            }
            Action::WorkRun { work, stage } => {
                let work = work.or_else(|| {
                    // The work the open page is in: site://<pipeline>/<work>/….
                    let url = &self.page.current.as_ref()?.url;
                    let parts: Vec<&str> = url.strip_prefix("site://")?.split('/').collect();
                    (parts.len() >= 3).then(|| format!("{}/{}", parts[0], parts[1]))
                });
                let Some(work) = work else {
                    self.ui
                        .flash("which work? :work run <pipeline>/<work>", FlashLevel::Warn);
                    return vec![];
                };
                self.send(
                    ClientCommand::WorkRun {
                        work,
                        stage,
                        session: self.session.current.clone(),
                    },
                    Pending::WorkRun,
                )
            }
            Action::Scope(scope) => {
                self.session.scope = scope.filter(|s| s != "none");
                let text = match &self.session.scope {
                    Some(scope) => format!("sessions narrowed to {scope} (:scope none for all)"),
                    None => "all sessions".to_string(),
                };
                self.ui.flash(text, FlashLevel::Info);
                vec![]
            }
            Action::ProjectsLoaded { query, projects } => {
                self.picker.projects(&query, projects);
                vec![]
            }
            Action::NewSessionIn(project) => self.send(
                ClientCommand::SessionCreate {
                    agent: self.default_agent.clone(),
                    project: Some(project),
                },
                Pending::CreateSession,
            ),
            Action::RefreshChanges => self.refresh_changes(),
            Action::TicketMerge(ticket) => {
                let Some(ticket) =
                    ticket.or_else(|| self.session.current_info().and_then(|i| i.ticket.clone()))
                else {
                    self.ui
                        .flash("not a worker session: :merge <ticket>", FlashLevel::Warn);
                    return vec![];
                };
                self.ui
                    .flash(format!("merging ticket {ticket}…"), FlashLevel::Info);
                self.send(
                    ClientCommand::TicketMerge {
                        ticket: ticket.clone(),
                    },
                    Pending::Merged(ticket),
                )
            }
            Action::TicketDiscard(ticket) => self.send(
                ClientCommand::TicketDiscard {
                    ticket: ticket.clone(),
                },
                Pending::Discarded(ticket),
            ),
            Action::DiffLoaded(diff) => {
                self.project.loaded(diff);
                vec![]
            }
            Action::DiffFailed(error) => {
                self.project.failed(error);
                vec![]
            }
            Action::OpenFileAt { path, line } => {
                match self.session.current_info().and_then(|i| i.project.clone()) {
                    Some(project) => vec![Effect::OpenFile {
                        path: format!("{project}/{path}"),
                        line,
                    }],
                    None => vec![],
                }
            }
            Action::ToggleFollow => {
                self.page.follow = !self.page.follow;
                let text = if self.page.follow {
                    "following the agent"
                } else {
                    "not following the agent"
                };
                self.ui.flash(text, FlashLevel::Info);
                vec![]
            }
            Action::OpenApp(site) => {
                let from_page = || {
                    let url = self.page.current.as_ref()?.url.clone();
                    url.strip_prefix("site://")?
                        .split('/')
                        .next()
                        .map(str::to_string)
                };
                let Some(site) = site.or_else(from_page) else {
                    self.ui.flash(
                        "open a page of the site first, or :app <site>",
                        FlashLevel::Warn,
                    );
                    return vec![];
                };
                let session = self.session.current.clone().unwrap_or_default();
                let url = format!(
                    "{}/{site}/?token={}&session={session}",
                    self.conn.server_url, self.conn.token
                );
                vec![Effect::CheckApp { site, url }]
            }
            Action::EditPage => match &self.page.current {
                Some(page) => vec![Effect::EditPage(page.url.clone())],
                None => vec![],
            },

            Action::ApprovalDecide(decision) => {
                let Some(approval) = self.approvals.current().cloned() else {
                    return vec![];
                };
                // Closed right away; the server's `approval.resolved` confirms it.
                self.approvals.resolve(&approval.request);
                self.send(
                    ClientCommand::ApprovalRespond {
                        request: approval.request,
                        decision,
                    },
                    Pending::Ignore,
                )
            }
            Action::ApprovalView => match self.approvals.current() {
                Some(a) => vec![Effect::ViewText {
                    name: match a.kind {
                        ApprovalKind::Recipe => format!("{}-{}.just", a.site, a.recipe),
                        ApprovalKind::Merge => format!("merge-{}.txt", a.request),
                        ApprovalKind::Tool => format!("tool-{}.txt", a.request),
                        ApprovalKind::Mcp => format!("mcp-{}.json", a.request),
                        ApprovalKind::Amendment => format!("amendment-{}.diff", a.request),
                    },
                    text: a.body.clone(),
                }],
                None => vec![],
            },
            Action::OpenPicker(PickerKind::Orchestration)
                if self.session.orchestration().is_empty() =>
            {
                self.no_orchestration();
                vec![]
            }
            Action::OpenPicker(kind) => {
                self.ui.picker_open = true;
                self.picker.open(kind, &self.session)
            }
            Action::PickerQuery(query) => self.picker.set_query(query),
            Action::PickerMove(delta) => {
                self.picker.move_selection(delta);
                vec![]
            }
            Action::PickerAccept => {
                self.ui.picker_open = false;
                match self.picker.selected_target() {
                    Some(PickerTarget::Url(url)) => self.reduce(Action::OpenUrl(url)),
                    Some(PickerTarget::Session(id)) => self.attach(&id, true),
                    Some(PickerTarget::Project(folder)) => {
                        self.reduce(Action::ProjectView(Some(folder)))
                    }
                    None => vec![],
                }
            }
            Action::ClosePicker => {
                self.ui.picker_open = false;
                vec![]
            }

            Action::Connected => self.connected(),
            Action::Disconnected(reason) => {
                self.conn.state = ConnState::Disconnected(reason);
                vec![]
            }
            Action::Server(message) => self.server_message(message),
            Action::PageLoaded(page) => {
                self.page.loaded(page);
                vec![]
            }
            Action::PageFailed { url, error } => {
                self.page.failed(&url, error.clone());
                self.ui.flash(error, FlashLevel::Error);
                vec![]
            }
            Action::SearchResults { query, hits } => {
                self.picker.search_results(&query, hits);
                vec![]
            }
            Action::SitesLoaded(sites) => {
                self.picker.sites(sites);
                vec![]
            }
            Action::CatalogLoaded { sites, pages } => {
                self.catalog = Catalog { sites, pages };
                vec![]
            }
            Action::AppChecked { site, url, sites } => {
                let found = sites
                    .as_ref()
                    .map(|all| all.iter().find(|s| s.name == site));
                let problem = match found {
                    // Couldn't ask (an older server, a network error): let the browser show it.
                    None => None,
                    Some(None) => Some(format!("there is no site://{site}")),
                    Some(Some(s)) if !s.has_ui => Some(format!(
                        "site://{site} has no app (no {site}/ui/ folder); new_app creates one"
                    )),
                    Some(Some(s)) if !s.app_built => Some(format!(
                        "site://{site}'s app is not built ({site}/ui/dist/index.html is missing): \
                         run its ui-build action"
                    )),
                    Some(Some(_)) => None,
                };
                match problem {
                    Some(text) => {
                        self.ui.flash(text, FlashLevel::Warn);
                        vec![]
                    }
                    None => vec![Effect::OpenBrowser(url)],
                }
            }
        }
    }

    fn refresh_changes(&mut self) -> Vec<Effect> {
        match self.session.current.clone() {
            Some(session) if self.project.open && !self.project.loading => {
                self.project.loading = true;
                vec![Effect::FetchDiff(session)]
            }
            _ => vec![],
        }
    }

    fn send(&mut self, command: ClientCommand, pending: Pending) -> Vec<Effect> {
        if !self.conn.is_connected() {
            self.ui
                .flash("not connected to the server", FlashLevel::Error);
            return vec![];
        }
        vec![self.conn.request(Outgoing { command, pending })]
    }

    fn attach(&mut self, id: &str, remember: bool) -> Vec<Effect> {
        // Uploads belong to the session they went to.
        if self.session.current.as_deref() != Some(id) && !self.attachments.files.is_empty() {
            self.attachments.clear();
            self.ui.flash(
                "attached files dropped: they belonged to the other session",
                FlashLevel::Info,
            );
        }
        self.session.begin_attach(id, remember);
        self.send(
            ClientCommand::SessionAttach {
                session: id.to_string(),
                since: None,
            },
            Pending::Attach(id.to_string()),
        )
    }

    fn no_orchestration(&mut self) {
        self.ui.flash(
            "this session has no workers or orchestrator · space w lists every session",
            FlashLevel::Info,
        );
    }

    fn connected(&mut self) -> Vec<Effect> {
        self.conn.state = ConnState::Connected;
        let mut effects = self.send(
            ClientCommand::Subscribe {
                urls: vec!["*".into()],
            },
            Pending::Ignore,
        );
        effects.extend(self.send(ClientCommand::SessionList, Pending::ListSessions));
        // After a reconnect, catch up on what we missed.
        if let Some(current) = self.session.current.clone() {
            let since = Some(self.session.last_seq);
            effects.extend(self.send(
                ClientCommand::SessionAttach {
                    session: current.clone(),
                    since,
                },
                Pending::Attach(current),
            ));
        }
        effects
    }

    fn server_message(&mut self, message: ServerMessage) -> Vec<Effect> {
        match message {
            ServerMessage::Ok { id, result } => self.reply(id, result),
            ServerMessage::Error { id, message } => {
                if let Some(id) = id {
                    self.conn.take_pending(id);
                }
                self.ui.flash(message, FlashLevel::Error);
                vec![]
            }
            ServerMessage::StageEnded {
                work,
                title,
                stage,
                status,
                ..
            } => {
                let site = work
                    .trim_start_matches("site://")
                    .split('/')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let stage = stage.replacen('_', " ", 1);
                let (text, level) = match status.as_str() {
                    "review" => (
                        format!("{title} · {stage} waits for review (:app {site} for the board)"),
                        FlashLevel::Warn,
                    ),
                    "failed" => (format!("{title} · {stage} failed"), FlashLevel::Error),
                    other => (format!("{title} · {stage} {other}"), FlashLevel::Info),
                };
                self.ui.flash(text, level);
                if self.project_view.open {
                    return self.reduce(Action::ProjectView(self.project_view.project.clone()));
                }
                vec![]
            }
            ServerMessage::SessionUpdated { session } => {
                self.session.upsert(session);
                vec![]
            }
            ServerMessage::Slots {
                used,
                total,
                waiting,
            } => {
                (
                    self.conn.slots_used,
                    self.conn.slots_total,
                    self.conn.slots_waiting,
                ) = (used, total, waiting);
                vec![]
            }
            ServerMessage::ConfigReloaded {
                restart_needed,
                error,
            } => {
                let (text, level) = config_reload_line(true, &restart_needed, error.as_deref());
                self.ui.flash(text, level);
                vec![]
            }
            ServerMessage::PageChanged { url, .. } if self.page.shows(&url) => {
                self.page.reload().into_iter().collect()
            }
            ServerMessage::PageChanged { .. } => vec![],
            ServerMessage::ReminderDue {
                url,
                title,
                due,
                late,
                session,
            } => {
                let when = if late { "was due" } else { "due" };
                let path = url.strip_prefix("site://").unwrap_or(&url);
                let path = path.strip_suffix(".md").unwrap_or(path);
                let next = match session {
                    Some(session) => format!(":attach {session}"),
                    None => format!(":open {path}"),
                };
                self.ui.flash(
                    format!("⏰ {title} ({when} {due}) · {next}"),
                    FlashLevel::Warn,
                );
                vec![]
            }
            ServerMessage::WorkerStatus {
                worker,
                ticket,
                parent,
                state,
            } => {
                self.session.workers.insert(worker.clone(), state);
                let (text, level) = match state {
                    composer_protocol::WorkerState::Starting => (
                        format!("worker #{worker} started on ticket {ticket} · space w to watch"),
                        FlashLevel::Info,
                    ),
                    composer_protocol::WorkerState::Running => return vec![],
                    composer_protocol::WorkerState::Done => (
                        format!("worker #{worker} finished ticket {ticket}"),
                        FlashLevel::Info,
                    ),
                    composer_protocol::WorkerState::Failed => (
                        format!("worker #{worker} failed ticket {ticket}"),
                        FlashLevel::Warn,
                    ),
                    composer_protocol::WorkerState::Paused => (
                        format!("worker #{worker} paused ticket {ticket} (usage limit)"),
                        FlashLevel::Warn,
                    ),
                    composer_protocol::WorkerState::Stuck => (
                        format!(
                            "worker #{worker} reached its turn limit on ticket {ticket}; its \
                             orchestrator decides whether it goes on"
                        ),
                        FlashLevel::Warn,
                    ),
                };
                // Watching it finish: back to the orchestrator, which gets its result next.
                // Not while you're typing, or the message would go to the orchestrator.
                let finished = matches!(
                    state,
                    composer_protocol::WorkerState::Done
                        | composer_protocol::WorkerState::Failed
                        | composer_protocol::WorkerState::Paused
                );
                if finished
                    && self.session.current.as_deref() == Some(worker.as_str())
                    && self.ui.mode != Mode::Insert
                {
                    let effects = self.attach(&parent, true);
                    self.ui.flash(
                        format!("{text} · back to the orchestrator (ctrl-o: the worker)"),
                        level,
                    );
                    return effects;
                }
                self.ui.flash(text, level);
                vec![]
            }
            ServerMessage::ApprovalRequest {
                request,
                session,
                kind,
                title,
                site,
                recipe,
                args,
                body,
                hash,
                rule,
            } => {
                self.approvals.add(PendingApproval {
                    request,
                    session,
                    kind,
                    title,
                    site,
                    recipe,
                    args,
                    body,
                    hash,
                    rule,
                });
                vec![]
            }
            ServerMessage::ApprovalResolved { request, .. } => {
                self.approvals.resolve(&request);
                vec![]
            }
            ServerMessage::AgentNavigated { ref url, .. } => {
                let url = url.clone();
                let current = self.session.apply(&message);
                if current && self.page.follow {
                    vec![self.page.navigate(url)]
                } else {
                    vec![]
                }
            }
            other => {
                let current = self.session.apply(&other);
                // The agent may have changed project files: refresh an open changes view.
                let changed = matches!(
                    other,
                    ServerMessage::ToolResult { .. } | ServerMessage::TurnDone { .. }
                );
                if current && changed && self.project.open {
                    self.refresh_changes()
                } else {
                    vec![]
                }
            }
        }
    }

    fn reply(&mut self, id: u64, result: CommandResult) -> Vec<Effect> {
        match (self.conn.take_pending(id), result) {
            (Pending::ListSessions, CommandResult::Sessions { sessions }) => {
                for info in sessions.into_iter().rev() {
                    self.session.upsert(info);
                }
                self.run_startup()
            }
            (Pending::CreateSession, CommandResult::Session { session }) => {
                let id = session.id.clone();
                self.session.upsert(session);
                self.attach(&id, true)
            }
            (Pending::Attach(_), CommandResult::Session { session }) => {
                self.session.upsert(session);
                vec![]
            }
            (Pending::Harnesses, CommandResult::Harnesses { harnesses, .. }) => {
                let states: Vec<String> = harnesses
                    .iter()
                    .map(|h| match (h.allowed, h.on) {
                        (false, _) => format!("{}: not allowed in config.toml", h.entry),
                        (true, on) => format!("{} {}", h.entry, if on { "on" } else { "off" }),
                    })
                    .collect();
                self.ui.flash(states.join(" · "), FlashLevel::Info);
                vec![]
            }
            (Pending::McpServers, CommandResult::McpServers { servers }) => {
                let (text, level) = mcp_summary(&servers);
                self.ui.flash(text, level);
                vec![]
            }
            (
                Pending::ReloadConfig,
                CommandResult::ConfigReloaded {
                    changed,
                    restart_needed,
                },
            ) => {
                // A change was also broadcast, and that flash says it.
                if !changed {
                    let (text, level) = config_reload_line(false, &restart_needed, None);
                    self.ui.flash(text, level);
                }
                vec![]
            }
            (Pending::Overview, CommandResult::Project { overview }) => {
                self.project_view.loaded(overview);
                vec![]
            }
            (
                Pending::Pipelines,
                CommandResult::Pipelines {
                    pipelines,
                    templates,
                    ..
                },
            ) => {
                let made: Vec<String> = pipelines
                    .iter()
                    .map(|p| format!("{} ({} stages, {} works)", p.site, p.stages.len(), p.works))
                    .collect();
                let kinds: Vec<&str> = templates.iter().map(|t| t.site.as_str()).collect();
                let text = if made.is_empty() {
                    format!("no pipelines yet: :pipeline new <{}>", kinds.join("|"))
                } else {
                    format!(
                        "pipelines: {} · templates: {}",
                        made.join(", "),
                        kinds.join(", ")
                    )
                };
                self.ui.flash(text, FlashLevel::Info);
                vec![]
            }
            (Pending::Pipeline, CommandResult::Pipeline { pipeline }) => {
                self.ui.flash(
                    format!(
                        "created site://{}: :work new {} <name> starts a piece of work",
                        pipeline.site, pipeline.site
                    ),
                    FlashLevel::Info,
                );
                self.project_view.open = false;
                self.reduce(Action::OpenUrl(format!("site://{}", pipeline.site)))
            }
            (Pending::Work, CommandResult::Work { work }) => {
                self.ui.flash(
                    format!("created {}: write its brief (e), then run 01", work.url),
                    FlashLevel::Info,
                );
                self.project_view.open = false;
                self.reduce(Action::OpenUrl(work.url))
            }
            (Pending::WorkRun, CommandResult::Work { work }) => {
                self.ui.flash(
                    format!(
                        "{}: {} (its worker shows under this session)",
                        work.url.trim_end_matches("/index.md"),
                        work.status.unwrap_or_default()
                    ),
                    FlashLevel::Info,
                );
                if self.project_view.open {
                    return self.reduce(Action::ProjectView(self.project_view.project.clone()));
                }
                vec![]
            }
            (Pending::Merged(ticket), CommandResult::Merged { commit }) => {
                self.ui.flash(
                    format!("ticket {ticket} merged as {commit}; worktree and branch removed"),
                    FlashLevel::Info,
                );
                self.refresh_changes()
            }
            (Pending::Discarded(ticket), CommandResult::Done) => {
                self.ui.flash(
                    format!("ticket {ticket} discarded: worktree and branch removed"),
                    FlashLevel::Info,
                );
                self.refresh_changes()
            }
            _ => vec![],
        }
    }

    fn run_startup(&mut self) -> Vec<Effect> {
        let Some(startup) = self.startup.take() else {
            return vec![];
        };
        if let Some(id) = startup.session {
            return self.attach(&id, false);
        }
        let latest = self
            .session
            .sessions
            .iter()
            .find(|s| s.parent.is_none())
            .map(|s| s.id.clone());
        match latest {
            Some(id) if !startup.new => self.attach(&id, false),
            _ => self.send(
                ClientCommand::SessionCreate {
                    agent: startup.agent,
                    project: startup.project,
                },
                Pending::CreateSession,
            ),
        }
    }

    /// Mode helper for components.
    pub fn mode(&self) -> Mode {
        self.ui.mode
    }
}

/// The line for a config reload: done, unchanged, didn't parse, or done but some of it needs a
/// restart of the server.
fn config_reload_line(
    changed: bool,
    restart_needed: &[String],
    error: Option<&str>,
) -> (String, FlashLevel) {
    if let Some(error) = error {
        let error = error.lines().next().unwrap_or(error);
        return (
            format!("config.toml: {error}; the server keeps the config it had"),
            FlashLevel::Error,
        );
    }
    if !changed {
        return ("config.toml: no changes".into(), FlashLevel::Info);
    }
    if restart_needed.is_empty() {
        return ("config.toml reloaded".into(), FlashLevel::Info);
    }
    (
        format!(
            "config.toml reloaded; restart `composer serve` for {}",
            restart_needed.join(", ")
        ),
        FlashLevel::Warn,
    )
}

/// `:mcp`'s line: each server's state, tool count and agents; a warning when one failed.
fn mcp_summary(servers: &[composer_protocol::http::McpServerInfo]) -> (String, FlashLevel) {
    if servers.is_empty() {
        return (
            "no MCP servers: add [mcp.<name>] to config.toml, then list its tools in an agent's \
             tools: (<name>__*)"
                .into(),
            FlashLevel::Info,
        );
    }
    // The ones that work first: a long error at the end is what gets cut.
    let mut servers: Vec<&composer_protocol::http::McpServerInfo> = servers.iter().collect();
    servers.sort_by_key(|s| match s.state.as_str() {
        "ready" => 0,
        "failed" => 2,
        _ => 1,
    });
    let parts: Vec<String> = servers
        .iter()
        .map(|s| {
            let agents = if s.agents.is_empty() {
                "no agent lists it".to_string()
            } else {
                format!("for {}", s.agents.join(", "))
            };
            match s.state.as_str() {
                "ready" => format!("{} ready, {} tools, {agents}", s.name, s.tools.len()),
                "failed" => format!(
                    "{} failed: {}",
                    s.name,
                    s.error.as_deref().unwrap_or("unknown error")
                ),
                state => format!("{} {state}", s.name),
            }
        })
        .collect();
    let level = if servers.iter().any(|s| s.state == "failed") {
        FlashLevel::Warn
    } else {
        FlashLevel::Info
    };
    (format!("mcp: {}", parts.join(" · ")), level)
}

#[cfg(test)]
mod tests {
    use composer_protocol::http::McpServerInfo;
    use composer_protocol::{ChatMessage, Role, SessionInfo, TurnState};

    use super::*;
    use crate::store::types::ChatItem;

    fn info(id: &str, parent: Option<&str>) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            agent: "default".into(),
            project: None,
            ticket: None,
            parent: parent.map(Into::into),
            worker: None,
            state: TurnState::Idle,
            last_seq: 0,
            context_tokens: 0,
            preview: None,
            updated: String::new(),
        }
    }

    fn connected() -> (AppState, Vec<Effect>) {
        let mut state = AppState::new(
            Startup {
                agent: "default".into(),
                ..Startup::default()
            },
            160_000,
        );
        let effects = state.reduce(Action::Connected);
        (state, effects)
    }

    fn sent(effects: &[Effect]) -> Vec<(u64, ClientCommand)> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Send(r) => Some((r.id, r.command.clone())),
                _ => None,
            })
            .collect()
    }

    fn assistant(content: &str) -> ChatMessage {
        ChatMessage {
            role: Role::Assistant,
            content: content.into(),
            thinking: Some("hmm".into()),
            tool_calls: vec![],
            pages: vec![],
            images: vec![],
            attachments: vec![],
            source: None,
        }
    }

    #[test]
    fn startup_resumes_latest_top_level_session() {
        let (mut state, effects) = connected();
        let list_id = sent(&effects)
            .iter()
            .find(|(_, c)| *c == ClientCommand::SessionList)
            .unwrap()
            .0;
        let sessions = vec![info("w1", Some("o1")), info("o1", None)];
        let effects = state.reduce(Action::Server(ServerMessage::Ok {
            id: list_id,
            result: CommandResult::Sessions { sessions },
        }));
        assert_eq!(
            sent(&effects)[0].1,
            ClientCommand::SessionAttach {
                session: "o1".into(),
                since: None
            }
        );
        assert_eq!(state.session.current.as_deref(), Some("o1"));
    }

    #[test]
    fn startup_creates_session_when_none_exist() {
        let (mut state, effects) = connected();
        let list_id = sent(&effects)
            .iter()
            .find(|(_, c)| *c == ClientCommand::SessionList)
            .unwrap()
            .0;
        let effects = state.reduce(Action::Server(ServerMessage::Ok {
            id: list_id,
            result: CommandResult::Sessions { sessions: vec![] },
        }));
        assert!(matches!(
            sent(&effects)[0].1,
            ClientCommand::SessionCreate { .. }
        ));
    }

    #[test]
    fn streaming_then_done_and_stale_events_ignored() {
        let (mut state, _) = connected();
        state.session.upsert(info("s", None));
        state.reduce(Action::AttachSession("s".into()));
        let delta = |seq, text: &str| ServerMessage::MessageDelta {
            session: "s".into(),
            seq,
            text: text.into(),
        };
        state.reduce(Action::Server(delta(1, "Hel")));
        state.reduce(Action::Server(delta(2, "lo")));
        assert_eq!(state.session.streaming.as_ref().unwrap().content, "Hello");
        state.reduce(Action::Server(ServerMessage::MessageDone {
            session: "s".into(),
            seq: 3,
            message: assistant("Hello"),
        }));
        assert!(state.session.streaming.is_none());
        assert_eq!(
            state.session.items,
            [
                ChatItem::Thinking { text: "hmm".into() },
                ChatItem::Text {
                    text: "Hello".into()
                }
            ]
        );
        // A replayed (older) event is ignored; another session's events too.
        state.reduce(Action::Server(delta(2, "dup")));
        state.reduce(Action::Server(ServerMessage::MessageDelta {
            session: "other".into(),
            seq: 9,
            text: "x".into(),
        }));
        assert!(state.session.streaming.is_none());
    }

    #[test]
    fn agent_navigation_follows_until_you_navigate() {
        let (mut state, _) = connected();
        state.session.upsert(info("s", None));
        state.reduce(Action::AttachSession("s".into()));
        let nav = |seq, url: &str| {
            Action::Server(ServerMessage::AgentNavigated {
                session: "s".into(),
                seq,
                url: url.into(),
            })
        };
        assert_eq!(
            state.reduce(nav(1, "site://memory/index.md")),
            [Effect::FetchPage("site://memory/index.md".into())]
        );
        state.reduce(Action::OpenUrl("site://plans".into()));
        assert_eq!(state.ui.focus, Pane::Page);
        assert!(!state.page.follow);
        assert!(state.reduce(nav(2, "site://home/index.md")).is_empty());
    }

    #[test]
    fn picker_accepting_a_session_attaches_and_remembers_the_jump() {
        let (mut state, _) = connected();
        state.session.upsert(info("a", None));
        state.session.upsert(info("b", None));
        state.reduce(Action::AttachSession("a".into()));
        state.reduce(Action::OpenPicker(PickerKind::Sessions));
        state.reduce(Action::PickerQuery("#b".into()));
        let effects = state.reduce(Action::PickerAccept);
        assert_eq!(
            sent(&effects)[0].1,
            ClientCommand::SessionAttach {
                session: "b".into(),
                since: None
            }
        );
        assert!(!state.ui.picker_open);
        let effects = state.reduce(Action::JumpBack);
        assert_eq!(
            sent(&effects)[0].1,
            ClientCommand::SessionAttach {
                session: "a".into(),
                since: None
            }
        );
    }

    fn attached(effects: &[Effect]) -> Option<String> {
        sent(effects).into_iter().find_map(|(_, c)| match c {
            ClientCommand::SessionAttach { session, .. } => Some(session),
            _ => None,
        })
    }

    /// An orchestrator `o` with workers `w1` (idle) and `w2` (running); `x` is unrelated.
    fn orchestration() -> AppState {
        let (mut state, _) = connected();
        state.session.upsert(info("x", None));
        state.session.upsert(info("o", None));
        state.session.upsert(info("w1", Some("o")));
        state.session.upsert(SessionInfo {
            state: TurnState::Running,
            ..info("w2", Some("o"))
        });
        state
    }

    #[test]
    fn the_orchestration_picker_offers_the_likely_switch() {
        let mut state = orchestration();
        state.reduce(Action::AttachSession("o".into()));
        state.reduce(Action::OpenPicker(PickerKind::Orchestration));
        let rows: Vec<(&str, &str)> = state
            .picker
            .items
            .iter()
            .map(|i| (i.title.as_str(), i.right.as_str()))
            .collect();
        // Session list order is newest first; the orchestrator heads its workers.
        assert_eq!(
            rows,
            [
                ("default #o ◂", ""),
                ("  default #w2", "● running"),
                ("  default #w1", ""),
            ]
        );
        assert_eq!(state.picker.selected, 1, "the busy worker");
        assert_eq!(
            attached(&state.reduce(Action::PickerAccept)).as_deref(),
            Some("w2")
        );

        // From a worker, Enter goes back to the orchestrator.
        state.reduce(Action::OpenPicker(PickerKind::Orchestration));
        assert_eq!(state.picker.selected, 0);

        // A session without workers has nothing to show.
        state.reduce(Action::AttachSession("x".into()));
        state.ui.picker_open = false;
        state.reduce(Action::OpenPicker(PickerKind::Orchestration));
        assert!(!state.ui.picker_open);
    }

    #[test]
    fn stepping_through_the_orchestration_wraps() {
        let mut state = orchestration();
        state.reduce(Action::AttachSession("o".into()));
        let mut step = |delta| attached(&state.reduce(Action::CycleOrchestration(delta)));
        assert_eq!(step(1).as_deref(), Some("w2"));
        assert_eq!(step(1).as_deref(), Some("w1"));
        assert_eq!(step(1).as_deref(), Some("o"));
        assert_eq!(step(-1).as_deref(), Some("w1"));
    }

    #[test]
    fn a_finished_worker_hands_back_to_its_orchestrator() {
        let mut state = orchestration();
        let status = |worker: &str, state| {
            Action::Server(ServerMessage::WorkerStatus {
                worker: worker.into(),
                ticket: "0003".into(),
                parent: "o".into(),
                state,
            })
        };
        state.reduce(Action::AttachSession("o".into()));
        state.reduce(Action::AttachSession("w2".into()));
        // Another worker finishing doesn't move you.
        assert!(
            attached(&state.reduce(status("w1", composer_protocol::WorkerState::Done))).is_none()
        );
        assert_eq!(state.session.status(&info("w1", Some("o"))), "done");

        let effects = state.reduce(status("w2", composer_protocol::WorkerState::Done));
        assert_eq!(attached(&effects).as_deref(), Some("o"));
        assert!(
            state
                .ui
                .flash
                .as_ref()
                .unwrap()
                .text
                .contains("back to the orchestrator")
        );
        // ctrl-o returns to the worker.
        assert_eq!(
            attached(&state.reduce(Action::JumpBack)).as_deref(),
            Some("w2")
        );

        // Typing a message: stay put.
        state.reduce(Action::SetMode(Mode::Insert));
        let effects = state.reduce(status("w2", composer_protocol::WorkerState::Failed));
        assert!(attached(&effects).is_none());
        assert_eq!(state.session.current.as_deref(), Some("w2"));
    }

    #[test]
    fn nothing_is_sent_while_disconnected() {
        let (mut state, _) = connected();
        state.reduce(Action::Disconnected("gone".into()));
        state.session.current = Some("s".into());
        assert!(state.reduce(Action::SendMessage("hi".into())).is_empty());
        assert!(state.ui.flash.is_some());
    }

    #[test]
    fn approvals_queue_answer_and_close() {
        let (mut state, _) = connected();
        let request = |id: &str| {
            Action::Server(ServerMessage::ApprovalRequest {
                request: id.into(),
                session: "s".into(),
                kind: ApprovalKind::Recipe,
                title: String::new(),
                site: "tools".into(),
                recipe: "hello".into(),
                args: vec![],
                body: "hello:\n    echo hi".into(),
                hash: "abc".into(),
                rule: None,
            })
        };
        state.reduce(request("r1"));
        state.reduce(request("r2"));
        state.reduce(request("r1")); // replayed on re-attach: not duplicated
        assert_eq!(state.approvals.count(), 2);

        let effects = state.reduce(Action::ApprovalDecide(
            composer_protocol::ApprovalDecision::Approve,
        ));
        assert_eq!(
            sent(&effects)[0].1,
            ClientCommand::ApprovalRespond {
                request: "r1".into(),
                decision: composer_protocol::ApprovalDecision::Approve
            }
        );
        assert_eq!(state.approvals.current().unwrap().request, "r2");
        // Answered elsewhere (another client): closes here too.
        state.reduce(Action::Server(ServerMessage::ApprovalResolved {
            request: "r2".into(),
            session: "s".into(),
            approved: false,
        }));
        assert!(state.approvals.current().is_none());
    }

    #[test]
    fn an_app_opens_only_when_built() {
        let (mut state, _) = connected();
        let effects = state.reduce(Action::OpenApp(Some("orchestration".into())));
        let [Effect::CheckApp { site, url }] = effects.as_slice() else {
            panic!("{effects:?}")
        };
        assert_eq!(site, "orchestration");
        let site = |name: &str, has_ui, app_built| composer_protocol::http::SiteSummary {
            name: name.into(),
            title: name.into(),
            has_justfile: true,
            has_ui,
            app_built,
            description: String::new(),
            pages: 0,
        };
        let mut check = |sites| {
            state.reduce(Action::AppChecked {
                site: "orchestration".into(),
                url: url.clone(),
                sites,
            })
        };
        assert!(check(Some(vec![site("orchestration", true, false)])).is_empty());
        assert_eq!(
            check(Some(vec![site("orchestration", true, true)])),
            [Effect::OpenBrowser(url.clone())]
        );
        assert_eq!(check(None), [Effect::OpenBrowser(url.clone())]);
        assert!(check(Some(vec![site("orchestration", false, false)])).is_empty());
        assert!(check(Some(vec![])).is_empty());
        let flash = state.ui.flash.as_ref().unwrap();
        assert!(
            flash.text.contains("no site://orchestration"),
            "{}",
            flash.text
        );
    }

    #[test]
    fn config_reload_lines() {
        assert_eq!(
            config_reload_line(true, &[], None),
            ("config.toml reloaded".into(), FlashLevel::Info)
        );
        assert_eq!(
            config_reload_line(false, &[], None).0,
            "config.toml: no changes"
        );
        let (text, level) = config_reload_line(true, &["provider".into()], None);
        assert_eq!(
            text,
            "config.toml reloaded; restart `composer serve` for provider"
        );
        assert_eq!(level, FlashLevel::Warn);
        let (text, level) = config_reload_line(true, &[], Some("TOML parse error at line 3\nmore"));
        assert_eq!(
            text,
            "config.toml: TOML parse error at line 3; the server keeps the config it had"
        );
        assert_eq!(level, FlashLevel::Error);
    }

    #[test]
    fn mcp_lines() {
        let server = |name: &str, state: &str, error: Option<&str>| McpServerInfo {
            name: name.into(),
            transport: "stdio".into(),
            state: state.into(),
            error: error.map(Into::into),
            tools: vec!["a__x".into(), "a__y".into()],
            agents: vec!["default".into()],
        };
        let (text, level) = mcp_summary(&[
            server("github", "ready", None),
            server("broken", "failed", Some("starting `x`: not found")),
            server("docs", "off", None),
        ]);
        assert_eq!(
            text,
            "mcp: github ready, 2 tools, for default · docs off · broken failed: starting `x`: not found"
        );
        assert_eq!(level, FlashLevel::Warn);
        assert!(mcp_summary(&[]).0.starts_with("no MCP servers"));
    }
}
