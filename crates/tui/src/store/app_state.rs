use composer_protocol::{ApprovalKind, ClientCommand, CommandResult, DeskTab, Seq, ServerMessage};

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

/// Messages a session opens with, and fetches each time you scroll up to the oldest.
const HISTORY_MESSAGES: u32 = 50;

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
    /// The project this client has open on the server (`project.open`): the current session's.
    opened_project: Option<String>,
    /// The desktop window this TUI is the chat of, and the session its stage was last told of.
    window: Option<String>,
    window_session: Option<String>,
    /// What the user points at in an app of the current session (the chip): goes with the next
    /// message unless `:unselect`.
    pub selection: Option<composer_protocol::Selection>,
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
            opened_project: None,
            window: startup.window.clone(),
            window_session: None,
            selection: None,
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
            Action::LoadOlder => match self.session.current.clone() {
                Some(session)
                    if self.session.loaded && self.session.more && !self.session.loading_older =>
                {
                    self.session.loading_older = true;
                    let before = Some(self.session.first_seq);
                    self.fetch_history(session, before)
                }
                _ => vec![],
            },
            Action::CopyText(text) => {
                let lines = text.lines().count();
                self.ui.flash(
                    format!("copied {lines} line{}", if lines == 1 { "" } else { "s" }),
                    FlashLevel::Info,
                );
                vec![Effect::CopyText(text)]
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
            // Also stops a paused worker for good (a usage limit, a restart cut it off).
            Action::CancelTurn => match self.session.current.clone() {
                Some(session) if self.session.is_busy() || self.session.is_paused_worker() => {
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

            Action::SetSite(site) => match self.session.current.clone() {
                Some(session) => {
                    self.send(ClientCommand::SiteSet { session, site }, Pending::SiteSet)
                }
                None => vec![],
            },
            Action::SetModel(model) => match self.session.current.clone() {
                Some(session) => self.send(
                    ClientCommand::ModelSet { session, model },
                    Pending::ModelSet,
                ),
                None => vec![],
            },
            Action::ShowModel => {
                let line = match self.session.current_info().and_then(|s| s.model.clone()) {
                    Some(model) => format!("model: {model} · :model none for the server's"),
                    None if self.catalog.default_model.is_empty() => "model: the server's".into(),
                    None => format!("model: {} (the server's)", self.catalog.default_model),
                };
                self.ui.flash(
                    format!("{line} · Tab after :model lists them"),
                    FlashLevel::Info,
                );
                // The list may have changed (another provider since).
                self.send(ClientCommand::ModelList, Pending::Models)
            }

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
                self.ui
                    .flash("connecting the MCP servers…", FlashLevel::Info);
                self.send(ClientCommand::McpList, Pending::McpServers)
            }
            Action::ReloadConfig => self.send(ClientCommand::ConfigReload, Pending::ReloadConfig),
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
            Action::DiffSemantic => {
                self.project.toggle_semantic();
                vec![]
            }
            Action::CommentAdd(comment) => {
                self.project.drafts.push(comment);
                vec![]
            }
            Action::CommentEdit { index, message } => {
                if let Some(draft) = self.project.drafts.get_mut(index) {
                    draft.message = message;
                }
                vec![]
            }
            Action::CommentRemove(index) => {
                if index < self.project.drafts.len() {
                    self.project.drafts.remove(index);
                }
                vec![]
            }
            Action::CommentsSend => {
                let ticket = self
                    .project
                    .diff
                    .as_ref()
                    .and_then(|d| d.ticket.as_ref())
                    .map(|t| t.ticket.clone());
                let Some(ticket) = ticket else {
                    self.ui.flash(
                        "comments go to a ticket's review: open a worker session's changes",
                        FlashLevel::Warn,
                    );
                    return vec![];
                };
                if self.project.drafts.is_empty() {
                    self.ui.flash("no comments to send", FlashLevel::Warn);
                    return vec![];
                }
                let comments = self.project.drafts.clone();
                self.send(
                    ClientCommand::TicketComment {
                        ticket: ticket.clone(),
                        comments,
                    },
                    Pending::Comments(ticket),
                )
            }
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
                // Opened with a token of its own (never the workspace's in its URL): on the
                // session's project if it is one of its apps, else on Home.
                let session = self.session.current.clone().unwrap_or_default();
                let project = self
                    .session
                    .current_info()
                    .and_then(|s| self.session.project_of(s));
                self.open_app(site, session, project)
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
                        ApprovalKind::Publish => format!("publish-{}.diff", a.request),
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

            Action::Connected => {
                // A new connection: the server has nothing open for it yet.
                self.opened_project = None;
                self.window_session = None;
                self.connected()
            }
            Action::Disconnected(reason) => {
                self.conn.state = ConnState::Disconnected(reason);
                vec![]
            }
            Action::Server(message) => {
                let mut effects = self.server_message(message);
                effects.extend(self.follow_project());
                effects.extend(self.follow_window());
                effects
            }
            Action::Unselect => match self.session.current.clone() {
                Some(session) if self.selection.take().is_some() => {
                    self.ui.flash("the selection is dropped", FlashLevel::Info);
                    self.send(ClientCommand::SelectionClear { session }, Pending::Ignore)
                }
                _ => {
                    self.ui.flash("nothing is selected", FlashLevel::Info);
                    vec![]
                }
            },
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
                (self.catalog.sites, self.catalog.pages) = (sites, pages);
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
        self.fetch_history(id.to_string(), None)
    }

    /// `session.history`: the session's last messages (`before` none), or those before it.
    fn fetch_history(&mut self, session: String, before: Option<Seq>) -> Vec<Effect> {
        self.send(
            ClientCommand::SessionHistory {
                session: session.clone(),
                before,
                messages: HISTORY_MESSAGES,
            },
            Pending::History { session, before },
        )
    }

    /// After its history (or an older server, without): what follows, live.
    fn attach_since(&mut self, session: String, since: Option<Seq>) -> Vec<Effect> {
        self.send(
            ClientCommand::SessionAttach {
                session: session.clone(),
                since,
            },
            Pending::Attach(session),
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
        effects.extend(self.send(ClientCommand::ModelList, Pending::Models));
        // A desktop window's TUI: the window's events (a session it picks) come here too.
        if let Some(window) = self.window.clone() {
            effects.extend(self.send(ClientCommand::WindowJoin { window }, Pending::Ignore));
        }
        // After a reconnect, catch up on what we missed (or, its history never came, start over).
        if let Some(current) = self.session.current.clone() {
            effects.extend(if self.session.loaded {
                let since = Some(self.session.last_seq);
                self.attach_since(current, since)
            } else {
                self.fetch_history(current, None)
            });
        }
        effects
    }

    fn open_app(&mut self, site: String, session: String, project: Option<String>) -> Vec<Effect> {
        self.send(
            ClientCommand::AppOpen {
                site: site.clone(),
                project: project.clone(),
                // The app messages this session only (ticket 0172).
                session: Some(session.clone()).filter(|s| !s.is_empty()),
            },
            Pending::OpenApp {
                site,
                session,
                project,
            },
        )
    }

    /// The window's stage shows this TUI's session (ticket 0177): tells the server when it changes.
    fn follow_window(&mut self) -> Vec<Effect> {
        let (Some(window), Some(session)) = (self.window.clone(), self.session.current.clone())
        else {
            return vec![];
        };
        if self.window_session.as_deref() == Some(session.as_str()) {
            return vec![];
        }
        self.window_session = Some(session.clone());
        self.selection = None;
        self.send(
            ClientCommand::WindowSession { window, session },
            Pending::Ignore,
        )
    }

    /// Keeps the current session's project open on the server (its apps' watches, `on_open`),
    /// closing the one before when it changes.
    fn follow_project(&mut self) -> Vec<Effect> {
        let current = self
            .session
            .current_info()
            .and_then(|s| self.session.project_of(s));
        if current == self.opened_project {
            return vec![];
        }
        let mut effects = Vec::new();
        if let Some(old) = self.opened_project.take() {
            effects.extend(self.send(
                ClientCommand::ProjectClose { project: old },
                Pending::Ignore,
            ));
        }
        if let Some(project) = current.clone() {
            effects.extend(self.send(ClientCommand::ProjectOpen { project }, Pending::Ignore));
        }
        self.opened_project = current;
        effects
    }

    fn server_message(&mut self, message: ServerMessage) -> Vec<Effect> {
        match message {
            ServerMessage::Ok { id, result } => self.reply(id, result),
            ServerMessage::Error { id, message } => {
                let pending = id.map(|id| self.conn.take_pending(id));
                match pending {
                    Some(Pending::Models) => vec![],
                    // Not one of the project's apps: on Home, then.
                    Some(Pending::OpenApp {
                        site,
                        session,
                        project: Some(_),
                    }) => self.open_app(site, session, None),
                    // An older server: the whole chat replays.
                    Some(Pending::History {
                        session,
                        before: None,
                    }) if self.session.current.as_deref() == Some(&session) => {
                        self.session.loaded = true;
                        self.attach_since(session, None)
                    }
                    Some(Pending::History { .. }) => {
                        self.session.loading_older = false;
                        self.session.more = false;
                        vec![]
                    }
                    _ => {
                        self.ui.flash(message, FlashLevel::Error);
                        vec![]
                    }
                }
            }
            // The desktop picked another session for this TUI's window: show it.
            ServerMessage::WindowChanged { window, session }
                if self.window.as_deref() == Some(window.as_str())
                    && self.session.current.as_deref() != Some(session.as_str()) =>
            {
                // Attaching then says so (`window.session`): the window hears it confirmed.
                self.attach(&session, true)
            }
            ServerMessage::SelectionChanged { session, selection }
                if self.session.current.as_deref() == Some(session.as_str()) =>
            {
                if let Some(chosen) = &selection {
                    self.ui.flash(
                        format!(
                            "◆ {} ({}): goes with your next message; :unselect drops it",
                            chosen.label, chosen.app
                        ),
                        FlashLevel::Info,
                    );
                }
                self.selection = selection;
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
            (Pending::OpenApp { site, session, .. }, CommandResult::AppOpened { opened }) => {
                // The desktop's stage is this TUI's other half: put the app on the session's desk
                // (opened by `:app`), and the stage shows it, instead of opening a browser. The
                // answer says whether it opened on a project or on Home.
                if self.window.is_some() {
                    return self.send(
                        ClientCommand::DeskOpen {
                            session,
                            tab: DeskTab::App {
                                site,
                                project: opened.project,
                            },
                            focus: true,
                        },
                        Pending::Ignore,
                    );
                }
                // The server knows the session it was opened from; `session` in the URL is for a
                // client app (the chat), which reads it from there.
                let join = if opened.url.contains('?') { '&' } else { '?' };
                let url = format!(
                    "{}{}{join}session={session}",
                    origin(&self.conn.server_url),
                    opened.url
                );
                vec![Effect::CheckApp { site, url }]
            }
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
            (Pending::History { session, before }, CommandResult::History { events, more })
                if self.session.current.as_deref() == Some(&session) =>
            {
                match before {
                    None if !self.session.loaded => {
                        self.session.history_loaded(&events, more);
                        let since = Some(self.session.last_seq);
                        self.attach_since(session, since)
                    }
                    Some(before) if before == self.session.first_seq => {
                        self.session.older_loaded(&events, more);
                        vec![]
                    }
                    _ => vec![],
                }
            }
            (Pending::Attach(_), CommandResult::Session { session }) => {
                self.session.upsert(session);
                vec![]
            }
            (
                Pending::Models,
                CommandResult::Models {
                    default_model,
                    models,
                },
            ) => {
                self.catalog.models = models;
                self.catalog.default_model = default_model;
                vec![]
            }
            (Pending::SiteSet, CommandResult::Note { text }) => {
                // The session's `site` follows from its `session.updated`.
                self.ui.flash(text, FlashLevel::Info);
                vec![]
            }
            (Pending::ModelSet, CommandResult::Session { session }) => {
                let line = match &session.model {
                    Some(model) => format!("model: {model}"),
                    None => format!("model: the server's ({})", self.catalog.default_model),
                };
                self.ui.flash(line, FlashLevel::Info);
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
            (Pending::Merged(ticket), CommandResult::Merged { commit }) => {
                self.ui.flash(
                    format!("ticket {ticket} merged as {commit}; worktree and branch removed"),
                    FlashLevel::Info,
                );
                self.refresh_changes()
            }
            (Pending::Comments(ticket), CommandResult::Comments { lines }) => {
                let sent = self.project.drafts.len();
                self.project.drafts.clear();
                self.project.comments = lines;
                self.ui.flash(
                    format!("{sent} comment(s) added to ticket {ticket}"),
                    FlashLevel::Info,
                );
                vec![]
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

/// `http://host:port` of a server URL (`http://host:port/w/default`): app URLs come with their
/// own path.
fn origin(server_url: &str) -> &str {
    let after = server_url.find("://").map_or(0, |i| i + 3);
    match server_url[after..].find('/') {
        Some(slash) => &server_url[..after + slash],
        None => server_url,
    }
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
            site: None,
            ticket: None,
            parent: parent.map(Into::into),
            worker: None,
            state: TurnState::Idle,
            last_seq: 0,
            context_tokens: 0,
            model: None,
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
    fn workflow_sends_site_set_and_flashes_the_outcome() {
        let (mut state, _) = connected();
        // No session shown yet: nothing to switch.
        assert!(state.reduce(Action::SetSite(Some("3d".into()))).is_empty());
        state.session.upsert(info("s1", None));
        state.session.current = Some("s1".into());

        let effects = state.reduce(Action::SetSite(Some("3d".into())));
        let [(id, ref command)] = sent(&effects)[..] else {
            panic!("{effects:?}")
        };
        assert_eq!(
            *command,
            ClientCommand::SiteSet {
                session: "s1".into(),
                site: Some("3d".into())
            }
        );
        state.reduce(Action::Server(ServerMessage::Ok {
            id,
            result: CommandResult::Note {
                text: "Entered 3d (3D). Started the blender application.".into(),
            },
        }));
        let flash = state.ui.flash.as_ref().unwrap();
        assert_eq!(
            flash.text,
            "Entered 3d (3D). Started the blender application."
        );
        assert_eq!(flash.level, FlashLevel::Info);

        // No name leaves; a refusal is the error flash.
        let effects = state.reduce(Action::SetSite(None));
        let [(id, ref command)] = sent(&effects)[..] else {
            panic!("{effects:?}")
        };
        assert_eq!(
            *command,
            ClientCommand::SiteSet {
                session: "s1".into(),
                site: None
            }
        );
        state.reduce(Action::Server(ServerMessage::Error {
            id: Some(id),
            message: "no site `nope`".into(),
        }));
        let flash = state.ui.flash.as_ref().unwrap();
        assert_eq!(
            (flash.text.as_str(), flash.level),
            ("no site `nope`", FlashLevel::Error)
        );
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
            ClientCommand::SessionHistory {
                session: "o1".into(),
                before: None,
                messages: HISTORY_MESSAGES
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
            ClientCommand::SessionHistory {
                session: "b".into(),
                before: None,
                messages: HISTORY_MESSAGES
            }
        );
        assert!(!state.ui.picker_open);
        let effects = state.reduce(Action::JumpBack);
        assert_eq!(
            sent(&effects)[0].1,
            ClientCommand::SessionHistory {
                session: "a".into(),
                before: None,
                messages: HISTORY_MESSAGES
            }
        );
    }

    #[test]
    fn a_window_tui_joins_its_window_and_shows_the_session_it_picks() {
        let mut state = AppState::new(
            Startup {
                agent: "default".into(),
                window: Some("w1".into()),
                ..Startup::default()
            },
            160_000,
        );
        let joined = state.reduce(Action::Connected);
        assert!(sent(&joined).iter().any(|(_, c)| *c
            == ClientCommand::WindowJoin {
                window: "w1".into()
            }));
        state.session.upsert(info("a", None));
        state.session.upsert(info("b", None));
        state.reduce(Action::AttachSession("a".into()));
        let changed = |window: &str, session: &str| {
            Action::Server(ServerMessage::WindowChanged {
                window: window.into(),
                session: session.into(),
            })
        };
        assert_eq!(
            attached(&state.reduce(changed("w2", "b"))),
            None,
            "another window"
        );
        assert_eq!(
            attached(&state.reduce(changed("w1", "a"))),
            None,
            "already showing it"
        );
        assert_eq!(
            attached(&state.reduce(changed("w1", "b"))).as_deref(),
            Some("b")
        );
    }

    /// The session opened: its history asked for (then it attaches), or attached.
    fn attached(effects: &[Effect]) -> Option<String> {
        sent(effects).into_iter().find_map(|(_, c)| match c {
            ClientCommand::SessionAttach { session, .. }
            | ClientCommand::SessionHistory {
                session,
                before: None,
                ..
            } => Some(session),
            _ => None,
        })
    }

    #[test]
    fn a_session_opens_on_its_last_messages_and_loads_older_ones_at_the_top() {
        let (mut state, _) = connected();
        state.session.upsert(info("s", None));
        let said = |seq, text: &str| ServerMessage::MessageDone {
            session: "s".into(),
            seq,
            message: assistant(text),
        };
        let effects = state.reduce(Action::AttachSession("s".into()));
        let [(id, ClientCommand::SessionHistory { before: None, .. })] = &sent(&effects)[..] else {
            panic!("{effects:?}")
        };
        let effects = state.reduce(Action::Server(ServerMessage::Ok {
            id: *id,
            result: CommandResult::History {
                events: vec![said(5, "c")],
                more: true,
            },
        }));
        assert!(matches!(
            &sent(&effects)[..],
            [(_, ClientCommand::SessionAttach { since: Some(5), .. })]
        ));
        assert!(state.session.loaded && state.session.more);

        let effects = state.reduce(Action::LoadOlder);
        let [
            (
                id,
                ClientCommand::SessionHistory {
                    before: Some(5), ..
                },
            ),
        ] = &sent(&effects)[..]
        else {
            panic!("{effects:?}")
        };
        assert!(state.reduce(Action::LoadOlder).is_empty(), "one at a time");
        state.reduce(Action::Server(said(7, "d")));
        state.reduce(Action::Server(ServerMessage::Ok {
            id: *id,
            result: CommandResult::History {
                events: vec![said(1, "a"), said(3, "b")],
                more: false,
            },
        }));
        let texts: Vec<_> = state
            .session
            .items
            .iter()
            .filter_map(|i| match i {
                ChatItem::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["a", "b", "c", "d"]);
        assert_eq!((state.session.first_seq, state.session.more), (1, false));
        assert!(state.reduce(Action::LoadOlder).is_empty(), "none left");
    }

    #[test]
    fn an_older_server_without_history_replays_the_whole_chat() {
        let (mut state, _) = connected();
        state.session.upsert(info("s", None));
        let effects = state.reduce(Action::AttachSession("s".into()));
        let id = sent(&effects)[0].0;
        let effects = state.reduce(Action::Server(ServerMessage::Error {
            id: Some(id),
            message: "unknown variant `session.history`".into(),
        }));
        assert!(matches!(
            &sent(&effects)[..],
            [(_, ClientCommand::SessionAttach { since: None, .. })]
        ));
        assert!(state.ui.flash.is_none());
    }

    /// A window's TUI tells the server which session its stage shows, once per change; without
    /// a window it says nothing. An app's selection is the chip; `:unselect` drops it.
    #[test]
    fn a_windows_tui_moves_its_stage_and_shows_the_chip() {
        let nudge = || {
            Action::Server(ServerMessage::Error {
                id: None,
                message: "x".into(),
            })
        };
        let windowed = |effects: &[Effect]| -> Vec<ClientCommand> {
            sent(effects)
                .into_iter()
                .map(|(_, c)| c)
                .filter(|c| matches!(c, ClientCommand::WindowSession { .. }))
                .collect()
        };
        let mut state = AppState::new(
            Startup {
                agent: "default".into(),
                window: Some("w1".into()),
                ..Startup::default()
            },
            160_000,
        );
        state.reduce(Action::Connected);
        state.session.upsert(info("a", None));
        state.session.upsert(info("b", None));
        state.reduce(Action::AttachSession("a".into()));
        assert_eq!(
            windowed(&state.reduce(nudge())),
            [ClientCommand::WindowSession {
                window: "w1".into(),
                session: "a".into()
            }]
        );
        assert!(windowed(&state.reduce(nudge())).is_empty(), "once");
        state.reduce(Action::AttachSession("b".into()));
        assert_eq!(windowed(&state.reduce(nudge())).len(), 1);

        // The chip, for the current session only.
        let chip = |session: &str| {
            Action::Server(ServerMessage::SelectionChanged {
                session: session.into(),
                selection: Some(composer_protocol::Selection {
                    app: "board".into(),
                    label: "ticket 0042".into(),
                    text: None,
                    url: None,
                    data: None,
                }),
            })
        };
        state.reduce(chip("a"));
        assert!(state.selection.is_none(), "another session's");
        state.reduce(chip("b"));
        assert_eq!(state.selection.as_ref().unwrap().label, "ticket 0042");
        let effects = state.reduce(Action::Unselect);
        assert!(state.selection.is_none());
        assert!(sent(&effects).iter().any(
            |(_, c)| matches!(c, ClientCommand::SelectionClear { session } if session == "b")
        ));

        // Without a window: nothing about windows.
        let (mut plain, _) = connected();
        plain.session.upsert(info("a", None));
        plain.reduce(Action::AttachSession("a".into()));
        assert!(windowed(&plain.reduce(nudge())).is_empty());
    }

    /// The current session's project is open on the server; switching closes it and opens the
    /// next; a reconnect opens it again.
    #[test]
    fn the_current_sessions_project_is_kept_open() {
        let (mut state, _) = connected();
        state.session.upsert(SessionInfo {
            project: Some("/p/game".into()),
            site: None,
            ..info("a", None)
        });
        state.session.upsert(SessionInfo {
            project: Some("/p/notes".into()),
            site: None,
            ..info("b", None)
        });
        let nudge = || {
            Action::Server(ServerMessage::Error {
                id: None,
                message: "x".into(),
            })
        };
        let projects = |effects: &[Effect]| -> Vec<ClientCommand> {
            sent(effects)
                .into_iter()
                .map(|(_, c)| c)
                .filter(|c| {
                    matches!(
                        c,
                        ClientCommand::ProjectOpen { .. } | ClientCommand::ProjectClose { .. }
                    )
                })
                .collect()
        };
        state.reduce(Action::AttachSession("a".into()));
        assert_eq!(
            projects(&state.reduce(nudge())),
            [ClientCommand::ProjectOpen {
                project: "/p/game".into()
            }]
        );
        assert!(projects(&state.reduce(nudge())).is_empty(), "once");
        state.reduce(Action::AttachSession("b".into()));
        assert_eq!(
            projects(&state.reduce(nudge())),
            [
                ClientCommand::ProjectClose {
                    project: "/p/game".into()
                },
                ClientCommand::ProjectOpen {
                    project: "/p/notes".into()
                },
            ]
        );
        state.reduce(Action::Connected);
        assert_eq!(
            projects(&state.reduce(nudge())),
            [ClientCommand::ProjectOpen {
                project: "/p/notes".into()
            }]
        );
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
    fn an_app_opens_by_its_own_token_on_the_project_else_home() {
        let (mut state, _) = connected();
        state.conn.server_url = "http://h:1/w/default".into();
        state.session.upsert(SessionInfo {
            project: Some("/p/game".into()),
            site: None,
            ..info("s", None)
        });
        state.reduce(Action::AttachSession("s".into()));
        let effects = state.reduce(Action::OpenApp(Some("calendar".into())));
        let commands: Vec<_> = sent(&effects)
            .into_iter()
            .filter(|(_, c)| matches!(c, ClientCommand::AppOpen { .. }))
            .collect();
        let [
            (
                id,
                ClientCommand::AppOpen {
                    site,
                    project,
                    session,
                },
            ),
        ] = commands.as_slice()
        else {
            panic!("{effects:?}")
        };
        assert_eq!(
            (site.as_str(), project.as_deref()),
            ("calendar", Some("/p/game"))
        );
        assert_eq!(
            session.as_deref(),
            Some("s"),
            "the session it is opened from"
        );
        // Not one of the project's apps: on Home.
        let effects = state.reduce(Action::Server(ServerMessage::Error {
            id: Some(*id),
            message: "isn't one of project `game`'s apps".into(),
        }));
        let commands: Vec<_> = sent(&effects)
            .into_iter()
            .filter(|(_, c)| matches!(c, ClientCommand::AppOpen { .. }))
            .collect();
        let [(id, ClientCommand::AppOpen { project: None, .. })] = commands.as_slice() else {
            panic!("{effects:?}")
        };
        let effects = state.reduce(Action::Server(ServerMessage::Ok {
            id: *id,
            result: CommandResult::AppOpened {
                opened: composer_protocol::http::AppOpened {
                    url: "/w/default/a/app_x/".into(),
                    token: Some("app_x".into()),
                    project: None,
                },
            },
        }));
        let [Effect::CheckApp { site, url }] = effects.as_slice() else {
            panic!("{effects:?}")
        };
        assert_eq!(site, "calendar");
        assert_eq!(url, "http://h:1/w/default/a/app_x/?session=s");
        assert!(!url.contains(&state.conn.token) || state.conn.token.is_empty());
    }

    #[test]
    fn a_windows_tui_puts_an_app_on_the_desk_not_in_a_browser() {
        let (mut state, _) = connected();
        state.window = Some("w1".into());
        state.conn.server_url = "http://h:1/w/default".into();
        state.session.upsert(SessionInfo {
            project: Some("/p/game".into()),
            site: None,
            ..info("s", None)
        });
        state.reduce(Action::AttachSession("s".into()));

        let open = |state: &mut AppState, project: Option<&str>| {
            let effects = state.reduce(Action::OpenApp(Some("calendar".into())));
            let commands = sent(&effects);
            let [(id, _)] = commands
                .iter()
                .filter(|(_, c)| matches!(c, ClientCommand::AppOpen { .. }))
                .collect::<Vec<_>>()
                .as_slice()
            else {
                panic!("{effects:?}")
            };
            let id = *id;
            let effects = state.reduce(Action::Server(ServerMessage::Ok {
                id,
                result: CommandResult::AppOpened {
                    opened: composer_protocol::http::AppOpened {
                        url: "/w/default/a/app_x/".into(),
                        token: Some("app_x".into()),
                        project: project.map(Into::into),
                    },
                },
            }));
            let commands = sent(&effects);
            assert!(
                effects
                    .iter()
                    .all(|e| !matches!(e, Effect::CheckApp { .. } | Effect::OpenBrowser(_))),
                "the stage shows apps, not a browser: {effects:?}"
            );
            commands
        };

        let desk_tab = |commands: &[(u64, ClientCommand)]| -> (String, DeskTab, bool) {
            commands
                .iter()
                .find_map(|(_, command)| match command {
                    ClientCommand::DeskOpen {
                        session,
                        tab,
                        focus,
                    } => Some((session.clone(), tab.clone(), *focus)),
                    _ => None,
                })
                .expect("a desk.open")
        };

        // On the session's project: the tab names it.
        let (session, tab, focus) = desk_tab(&open(&mut state, Some("game")));
        assert_eq!(session, "s");
        assert!(focus, "the stage shows the tab it was opened on");
        assert_eq!(
            tab,
            DeskTab::App {
                site: "calendar".into(),
                project: Some("game".into())
            }
        );

        // On Home: no project.
        let (_, tab, _) = desk_tab(&open(&mut state, None));
        assert_eq!(
            tab,
            DeskTab::App {
                site: "calendar".into(),
                project: None
            }
        );
    }

    #[test]
    fn an_app_opens_only_when_built() {
        let (mut state, _) = connected();
        let effects = state.reduce(Action::OpenApp(Some("orchestration".into())));
        let commands: Vec<_> = sent(&effects)
            .into_iter()
            .filter(|(_, c)| matches!(c, ClientCommand::AppOpen { .. }))
            .collect();
        let [(id, ClientCommand::AppOpen { .. })] = commands.as_slice() else {
            panic!("{effects:?}")
        };
        let effects = state.reduce(Action::Server(ServerMessage::Ok {
            id: *id,
            result: CommandResult::AppOpened {
                opened: composer_protocol::http::AppOpened {
                    url: "/a/app_x/".into(),
                    token: Some("app_x".into()),
                    project: None,
                },
            },
        }));
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
            app: None,
            mcp: Vec::new(),
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

    #[test]
    fn comments_are_sent_to_the_ticket_and_replaced_by_its_lines() {
        use composer_protocol::http::{DiffComment, ProjectDiff, TicketReview};
        let (mut state, _) = connected();
        let comment = DiffComment {
            file: "a.rs".into(),
            start: 2,
            end: 2,
            message: "why?".into(),
            quote: None,
        };
        // No ticket review open: nowhere to send them.
        state.reduce(Action::CommentAdd(comment.clone()));
        assert!(sent(&state.reduce(Action::CommentsSend)).is_empty());
        state.project.loaded(ProjectDiff {
            path: "/wt".into(),
            branch: None,
            files: vec![],
            diff: String::new(),
            ticket: Some(TicketReview {
                ticket: "0007".into(),
                branch: "ticket/0007".into(),
                target: None,
                ahead: 0,
                commits: vec![],
                uncommitted: false,
                conflicts: vec![],
            }),
        });
        assert!(
            state.project.drafts.is_empty(),
            "another review: its own comments"
        );
        state.reduce(Action::CommentAdd(comment.clone()));
        let effects = state.reduce(Action::CommentsSend);
        let (id, command) = sent(&effects).remove(0);
        assert_eq!(
            command,
            ClientCommand::TicketComment {
                ticket: "0007".into(),
                comments: vec![comment]
            }
        );
        state.reduce(Action::Server(ServerMessage::Ok {
            id,
            result: CommandResult::Comments {
                lines: vec!["- a.rs:2:2 — why?".into()],
            },
        }));
        assert!(state.project.drafts.is_empty());
        assert_eq!(state.project.comments, ["- a.rs:2:2 — why?"]);
    }
}
