use composer_protocol::{
    ApprovalKind, ClientCommand, CommandResult, DeskBy, DeskTab, RunKind, Seq, ServerMessage,
};

use crate::action::Action;
use crate::effect::Effect;
use crate::store::desk_store::{StageTab, page_key};
use crate::store::types::{
    Catalog, ConnState, FileSource, FlashLevel, Mode, Outgoing, Pane, Pending, PendingApproval,
    PickerTarget, Startup,
};
use crate::store::{
    ApprovalStore, AttachmentStore, ChangesStore, ConnectionStore, DeskStore, PageStore,
    PickerStore, SessionStore, TerminalStore, UiStore,
};

/// Messages a session opens with, and fetches each time you scroll up to the oldest.
const HISTORY_MESSAGES: u32 = 50;

/// All shared state. `reduce` is the only way it changes; it does no IO.
#[derive(Debug)]
pub struct AppState {
    pub conn: ConnectionStore,
    pub session: SessionStore,
    pub page: PageStore,
    /// The session's desk, as its owner sent it: the stage is its tabs a terminal can draw
    /// (plan phase 12, ticket 0208).
    pub desk: DeskStore,
    pub ui: UiStore,
    pub picker: PickerStore,
    pub approvals: ApprovalStore,
    pub changes: ChangesStore,
    pub catalog: Catalog,
    /// Files attached to the message being written.
    pub attachments: AttachmentStore,
    /// The server terminals on the stage, and the session's background processes.
    pub terminals: TerminalStore,
    startup: Option<Startup>,
    default_kind: RunKind,
}

impl AppState {
    pub fn new(startup: Startup, context_limit: u32) -> Self {
        Self {
            conn: ConnectionStore::new(context_limit),
            session: SessionStore::default(),
            page: PageStore::new(),
            desk: DeskStore::new(),
            ui: UiStore::new(),
            picker: PickerStore::new(),
            approvals: ApprovalStore::default(),
            changes: ChangesStore::default(),
            catalog: Catalog::default(),
            attachments: AttachmentStore::default(),
            terminals: TerminalStore::new(),
            default_kind: startup.kind,
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
            Action::CancelTurn => match self.session.current.clone() {
                Some(session) if self.session.is_busy() => {
                    // What it had seen: a turn that started since (a queued message) keeps going.
                    let seen = Some(self.session.last_seq).filter(|s| *s > 0);
                    self.send(ClientCommand::TurnCancel { session, seen }, Pending::Ignore)
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
            Action::NewSession { kind } => {
                let kind = kind.unwrap_or(self.default_kind);
                // The new chat starts in this session's site.
                let site = self.session.current_info().and_then(|s| s.site.clone());
                self.send(
                    ClientCommand::SessionCreate {
                        kind,
                        model: None,
                        site,
                        harness: None,
                    },
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

            Action::OpenUrl(url) => {
                if !url.starts_with("site://") {
                    self.ui
                        .flash(format!("external link: {url}"), FlashLevel::Info);
                    return vec![];
                }
                // You navigated yourself: stop following the agent and look at the page.
                self.page.follow = false;
                self.reduce(Action::FocusPane(Pane::Stage));
                self.open_page(&url, true)
            }
            Action::PageBack => self.step_page(true),
            Action::PageForward => self.step_page(false),
            Action::ReloadPage => match self.shown_tab() {
                Some(StageTab::Changes { .. }) => self.refresh_changes(),
                _ => self.page.reload().into_iter().collect(),
            },
            Action::ToggleChanges => match self.changes_tab() {
                Some(id) if self.matches_shown(|tab| tab.id() == id) => self.close_shown(),
                Some(id) => self.show_tab_id(&id),
                None => self.open_changes(),
            },
            Action::CycleTab(delta) => self.cycle_tab(delta),
            Action::ShowTab(index) => self.show_tab(index),
            Action::CloseTab => self.close_shown(),
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
            Action::RefreshChanges => self.refresh_changes(),
            Action::OpenTerminal(command) => self.open_terminal(None, command),
            Action::TerminalKey(key) => self.terminal_key(key),
            Action::TerminalPaste(text) => {
                if let Some(screen) = self.shown_screen() {
                    screen.paste(&text);
                }
                vec![]
            }
            Action::TerminalWrite { terminal, data } => self.send(
                ClientCommand::TerminalInput {
                    terminal,
                    data: String::from_utf8_lossy(&data).into_owned(),
                },
                Pending::Ignore,
            ),
            Action::TerminalScroll(lines) => {
                if let Some(screen) = self.shown_screen() {
                    screen.scroll_by(lines);
                }
                vec![]
            }
            Action::TerminalResize(cols, rows) => {
                self.terminals.size = (cols.max(2), rows.max(2));
                vec![]
            }
            Action::ListProcesses => {
                let text = match self.terminals.processes.as_slice() {
                    [] => "no background processes".to_string(),
                    processes => processes
                        .iter()
                        .map(|p| format!("{} `{}`", p.terminal, p.command))
                        .collect::<Vec<_>>()
                        .join(" \u{b7} "),
                };
                self.ui.flash(text, FlashLevel::Info);
                vec![]
            }
            Action::StopProcess(id) => self.stop_process(id),
            Action::DiffSemantic => {
                self.changes.toggle_semantic();
                vec![]
            }
            Action::DiffLoaded(diff) => {
                self.changes.loaded(diff);
                vec![]
            }
            Action::DiffFailed(error) => {
                self.changes.failed(error);
                vec![]
            }
            Action::OpenFileAt { path, line } => {
                // The diff names the folder it is of.
                match self.changes.diff.as_ref().map(|d| d.path.clone()) {
                    Some(folder) => vec![Effect::OpenFile {
                        path: format!("{folder}/{path}"),
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
                        ApprovalKind::Tool => format!("tool-{}.txt", a.request),
                        ApprovalKind::Mcp => format!("mcp-{}.json", a.request),
                    },
                    text: a.body.clone(),
                }],
                None => vec![],
            },
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
                let id = page_key(&page.url).unwrap_or_default();
                self.page.loaded(&id, page);
                vec![]
            }
            Action::PageFailed { url, error } => {
                let id = page_key(&url).unwrap_or_default();
                self.page.failed(&id, &url, error.clone());
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
        }
    }

    fn refresh_changes(&mut self) -> Vec<Effect> {
        match self.session.current.clone() {
            Some(session) if self.changes_open() && !self.changes.loading => {
                self.changes.loading = true;
                vec![Effect::FetchDiff(session)]
            }
            _ => vec![],
        }
    }

    // ---- the stage: the session's desk, drawn in this terminal ----

    /// The tabs the stage can draw: the desk's pages and changes, in the desk's order.
    pub fn stage_tabs(&self) -> Vec<StageTab> {
        let mut tabs = self.desk.visible();
        for tab in &mut tabs {
            if let StageTab::Terminal {
                terminal, label, ..
            } = tab
            {
                *label = self.terminals.process(terminal).map(|p| p.command.clone());
            }
        }
        tabs
    }

    /// The server terminal on the screen, if a terminal tab is shown.
    pub fn shown_terminal(&self) -> Option<String> {
        match self.shown_tab()? {
            StageTab::Terminal { terminal, .. } => Some(terminal),
            _ => None,
        }
    }

    /// Whether keys go to the terminal on the screen: it is shown, the stage has the focus,
    /// and nothing else (a mode, an overlay) has the keys.
    pub fn terminal_focused(&self) -> bool {
        self.ui.focus == Pane::Stage
            && self.ui.mode == Mode::Normal
            && !self.ui.picker_open
            && !self.ui.help_open
            && self.approvals.current().is_none()
            && self.shown_terminal().is_some()
    }

    fn shown_screen(&mut self) -> Option<&mut composer_term::TermScreen> {
        let id = self.shown_terminal()?;
        self.terminals.screen_mut(&id)
    }

    /// `terminal.open`: a new terminal (`command`: a program in it), or (`id`) a fresh shell
    /// where an ended one was. Shown when the desk says so.
    fn open_terminal(&mut self, id: Option<String>, command: Option<String>) -> Vec<Effect> {
        let Some(session) = self.session.current.clone() else {
            self.ui.flash("no session", FlashLevel::Warn);
            return vec![];
        };
        let (cols, rows) = self.terminals.size;
        let effects = self.send(
            ClientCommand::TerminalOpen {
                session,
                terminal: id,
                cols,
                rows,
                command,
            },
            Pending::TerminalOpen,
        );
        self.terminals.opening = !effects.is_empty();
        self.reduce(Action::FocusPane(Pane::Stage));
        effects
    }

    /// A key for the terminal on the screen: encoded by its screen (what it writes arrives as
    /// `TerminalWrite`); Enter on one that ended opens a fresh shell there.
    fn terminal_key(&mut self, key: crossterm::event::KeyEvent) -> Vec<Effect> {
        let Some(id) = self.shown_terminal() else {
            return vec![];
        };
        if self.terminals.ended.contains(&id) {
            if key.code == crossterm::event::KeyCode::Enter {
                return self.open_terminal(Some(id), None);
            }
            return vec![];
        }
        if let (Some(screen), Some((code, mods))) = (
            self.terminals.screen_mut(&id),
            crate::term_keys::term_key(key),
        ) {
            screen.key(code, mods);
        }
        vec![]
    }

    /// `:stop [id]`: the one named, else the one shown, else the only one.
    fn stop_process(&mut self, id: Option<String>) -> Vec<Effect> {
        let processes = &self.terminals.processes;
        let shown = self
            .shown_terminal()
            .filter(|id| processes.iter().any(|p| &p.terminal == id));
        let only = (processes.len() == 1).then(|| processes[0].terminal.clone());
        match id.or(shown).or(only) {
            Some(terminal) => self.send(ClientCommand::ProcessStop { terminal }, Pending::Ignore),
            None => {
                let text = if processes.is_empty() {
                    "no background processes"
                } else {
                    "which one? :stop <id> (:ps lists them)"
                };
                self.ui.flash(text, FlashLevel::Warn);
                vec![]
            }
        }
    }

    /// The session's background processes, asked for.
    fn fetch_processes(&mut self) -> Vec<Effect> {
        let Some(session) = self.session.current.clone() else {
            return vec![];
        };
        self.send(
            ClientCommand::ProcessList {
                session: session.clone(),
            },
            Pending::Processes { session },
        )
    }

    /// The ids of the desk's tabs a terminal can draw, in order.
    pub fn visible_ids(&self) -> Vec<String> {
        self.desk
            .visible()
            .iter()
            .map(|tab| tab.id().to_string())
            .collect()
    }

    /// Every tab id the stage could be showing.
    fn showable_ids(&self) -> Vec<String> {
        self.stage_tabs()
            .iter()
            .map(|tab| tab.id().to_string())
            .collect()
    }

    /// Which tab is on the screen, as an index into [`Self::stage_tabs`].
    pub fn shown_tab_index(&self) -> Option<usize> {
        let id = self.desk.shown()?;
        self.stage_tabs().iter().position(|tab| tab.id() == id)
    }

    /// The tab on the screen.
    pub fn shown_tab(&self) -> Option<StageTab> {
        let at = self.shown_tab_index()?;
        self.stage_tabs().into_iter().nth(at)
    }

    /// The changes tab that is open, if any.
    fn changes_tab(&self) -> Option<String> {
        self.desk
            .visible()
            .iter()
            .find(|tab| matches!(tab, StageTab::Changes { .. }))
            .map(|tab| tab.id().to_string())
    }

    /// Whether the tab on the screen is one `check` describes.
    fn matches_shown(&self, check: impl Fn(&StageTab) -> bool) -> bool {
        self.shown_tab().is_some_and(|tab| check(&tab))
    }

    /// Whether a changes tab is open: its diff is kept live while it is.
    pub fn changes_open(&self) -> bool {
        self.desk
            .visible()
            .iter()
            .any(|tab| matches!(tab, StageTab::Changes { .. }))
    }

    /// Keep the page on the screen the shown tab's page, and the shown tab the desk's. Called
    /// after every action: the desk may have changed, or a tab was opened locally.
    pub fn sync_stage(&mut self) -> Vec<Effect> {
        // The tab shown has to be one that can be drawn: the desk's pages and changes. A page
        // belongs to a desk tab, so only those are kept when dropping what is gone.
        let ids = self.showable_ids();
        self.desk.reconcile(&ids);
        self.page.forget(&self.visible_ids());
        let terminals: Vec<String> = self
            .stage_tabs()
            .into_iter()
            .filter_map(|tab| match tab {
                StageTab::Terminal { terminal, .. } => Some(terminal),
                _ => None,
            })
            .collect();
        self.terminals.keep(&terminals);
        let shown = self.shown_tab();
        match shown {
            Some(StageTab::Page { id, url, .. }) => {
                // Its page is either on its way or already failed and said so on the screen:
                // neither is asked for again on every keypress. `r` and re-showing the tab are.
                if self.page.waiting_on_shown() == Some(id.as_str()) {
                    return vec![];
                }
                match self.page.show(&id, &url) {
                    Some(Effect::FetchPage(url)) if self.conn.is_connected() => {
                        vec![Effect::FetchPage(url)]
                    }
                    _ => vec![],
                }
            }
            Some(StageTab::Terminal { terminal, .. }) => {
                self.page.hide();
                self.sync_terminal(&terminal)
            }
            Some(StageTab::Changes { .. }) => {
                self.page.hide();
                if self.changes.diff.is_none()
                    && self.changes.error.is_none()
                    && !self.changes.loading
                {
                    self.refresh_changes()
                } else {
                    vec![]
                }
            }
            None => {
                self.page.hide();
                vec![]
            }
        }
    }

    /// The terminal on the screen has a screen here (its scrollback asked for once) at the
    /// stage's size (the server told of a new one).
    fn sync_terminal(&mut self, id: &str) -> Vec<Effect> {
        let (cols, rows) = self.terminals.size;
        let ended = self.terminals.ended.contains(id);
        if let Some(screen) = self.terminals.screen_mut(id) {
            if screen.resize(cols as usize, rows as usize) && !ended {
                return self.send(
                    ClientCommand::TerminalResize {
                        terminal: id.to_string(),
                        cols,
                        rows,
                    },
                    Pending::Ignore,
                );
            }
            return vec![];
        }
        if self.terminals.reading.contains(id)
            || self.terminals.opening
            || self.terminals.ended.contains(id)
            || !self.conn.is_connected()
        {
            return vec![];
        }
        self.terminals.reading.insert(id.to_string());
        self.send(
            ClientCommand::TerminalRead {
                terminal: id.to_string(),
            },
            Pending::TerminalRead(id.to_string()),
        )
    }

    /// A page becomes a tab on the desk (the desk derives the id from the page, so the same page
    /// is one tab) and, with `focus`, the one on the screen. Then shown here and now; the desk
    /// confirms with `desk.changed`, which is what tells the desktop and the phone.
    fn open_page(&mut self, url: &str, focus: bool) -> Vec<Effect> {
        let mut effects = match self.session.current.clone() {
            Some(session) => self.send(
                ClientCommand::DeskOpen {
                    session,
                    tab: DeskTab::Page {
                        url: url.to_string(),
                    },
                    focus,
                },
                Pending::Ignore,
            ),
            None => vec![],
        };
        effects.extend(self.page.navigate(url));
        effects
    }

    /// The changes of the folder of the session's site as a tab (`gd`): opened on the desk and
    /// shown, or, when there is no session to put it on, only here.
    fn open_changes(&mut self) -> Vec<Effect> {
        let Some(site) = self.changes_site() else {
            self.ui.flash(
                "this session is in no site (`:site <name>`, one with a `folder:`)",
                FlashLevel::Warn,
            );
            return vec![];
        };
        let effects = self.send(
            ClientCommand::DeskOpen {
                session: self.session.current.clone().unwrap_or_default(),
                tab: DeskTab::Changes { site },
                focus: true,
            },
            Pending::Ignore,
        );
        self.reduce(Action::FocusPane(Pane::Stage));
        effects
    }

    /// The site whose folder a changes tab shows: this session's.
    fn changes_site(&self) -> Option<String> {
        self.session.current_info().and_then(|s| s.site.clone())
    }

    /// A changes tab naming a site other than this session's: the diff there is the folder of
    /// the session's site, so that tab has nothing of its own to show. Says which.
    pub fn changes_elsewhere(&self) -> Option<String> {
        let here = self.changes_site();
        match self.shown_tab() {
            Some(StageTab::Changes { site, .. }) if here.as_deref() != Some(site.as_str()) => {
                Some(site)
            }
            _ => None,
        }
    }

    /// Step through the pages you looked at (`H` / `L`), showing their tab too.
    fn step_page(&mut self, back: bool) -> Vec<Effect> {
        let step = if back {
            self.page.back()
        } else {
            self.page.forward()
        };
        let Some((id, _url, effect)) = step else {
            self.ui.flash(
                if back {
                    "no page before this one"
                } else {
                    "no page after this one"
                },
                FlashLevel::Info,
            );
            return vec![];
        };
        let mut effects = self.show_tab_id(&id);
        effects.extend(effect);
        effects
    }

    fn cycle_tab(&mut self, delta: i32) -> Vec<Effect> {
        let tabs = self.stage_tabs();
        if tabs.is_empty() {
            return vec![];
        }
        let at = self
            .shown_tab_index()
            .map(|at| (at as i32 + delta).rem_euclid(tabs.len() as i32) as usize)
            .unwrap_or(tabs.len() - 1);
        let id = tabs[at].id().to_string();
        self.reduce(Action::FocusPane(Pane::Stage));
        self.show_tab_id(&id)
    }

    fn show_tab(&mut self, index: usize) -> Vec<Effect> {
        let Some(tab) = self.stage_tabs().into_iter().nth(index) else {
            return vec![];
        };
        let id = tab.id().to_string();
        self.show_tab_id(&id)
    }

    /// Put a tab on the screen: on the desk when it is one of the desk's, and here either way
    /// (waiting for the desk to agree, so it does not feel like a key that did nothing).
    fn show_tab_id(&mut self, id: &str) -> Vec<Effect> {
        if !self.desk.can_draw(id) {
            return vec![];
        }
        self.desk.show(id);
        // The desk agrees with a `desk.changed`, which is also what tells the other clients.
        let effects = match self.session.current.clone() {
            Some(session) => self.send(
                ClientCommand::DeskFocus {
                    session,
                    tab: id.to_string(),
                },
                Pending::Ignore,
            ),
            None => vec![],
        };
        let mut effects = self.show_local(id, effects);
        effects.extend(self.refresh_changes());
        effects
    }

    /// Show tab `id`'s content here (the desk's `shown` is what it agrees to).
    fn show_local(&mut self, id: &str, mut effects: Vec<Effect>) -> Vec<Effect> {
        self.desk.show(id);
        match self.desk.visible().into_iter().find(|tab| tab.id() == id) {
            Some(StageTab::Page { url, .. }) => {
                if let Some(effect) = self.page.show(id, &url) {
                    effects.push(effect);
                }
            }
            Some(StageTab::Changes { .. } | StageTab::Terminal { .. }) => {
                self.page.hide();
            }
            None => self.page.hide(),
        }
        effects
    }

    /// Close the tab on the screen. Its neighbour is shown, the same rule the desk follows, and
    /// the stage falls back to it here so the screen never waits for the reply.
    fn close_shown(&mut self) -> Vec<Effect> {
        let Some(tab) = self.shown_tab() else {
            self.ui.flash("nothing to close", FlashLevel::Info);
            return vec![];
        };
        let id = tab.id().to_string();
        let effects = match self.session.current.clone() {
            Some(session) => {
                let tab = id.clone();
                self.send(ClientCommand::DeskClose { session, tab }, Pending::Ignore)
            }
            None => vec![],
        };
        // The tab is gone from the screen already, its neighbour shown; the desk catching up
        // with the close changes nothing here.
        self.desk.closing(&id);
        let ids = self.showable_ids();
        self.desk.reconcile(&ids);
        self.page.forget(&self.visible_ids());
        let next = self.desk.shown().unwrap_or("").to_string();
        self.show_local(&next, effects)
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
        // What was open belongs to the session you were on. This one's tabs come from it, and
        // the `desk.get` answer below is what fills the stage: the server does not send a desk
        // unasked and does not broadcast one on attach.
        self.desk.clear();
        self.page.forget(&[]);
        self.terminals.clear();
        let mut effects = self.fetch_history(id.to_string(), None);
        effects.extend(self.send(
            ClientCommand::ProcessList {
                session: id.to_string(),
            },
            Pending::Processes {
                session: id.to_string(),
            },
        ));
        effects.extend(self.send(
            ClientCommand::DeskGet {
                session: id.to_string(),
            },
            Pending::Desk {
                session: id.to_string(),
            },
        ));
        effects
    }

    /// The session's desk, asked for after a reconnect (the first one comes with `attach`).
    fn fetch_desk(&mut self) -> Vec<Effect> {
        let session = self.session.current.clone().unwrap_or_default();
        self.send(
            ClientCommand::DeskGet {
                session: session.clone(),
            },
            Pending::Desk { session },
        )
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
        // After a reconnect, catch up on what we missed (or, its history never came, start over).
        if let Some(current) = self.session.current.clone() {
            effects.extend(if self.session.loaded {
                let since = Some(self.session.last_seq);
                self.attach_since(current, since)
            } else {
                self.fetch_history(current, None)
            });
            effects.extend(self.fetch_desk());
            // What the terminals printed meanwhile: read again.
            self.terminals.clear();
            effects.extend(self.fetch_processes());
        }
        effects
    }

    fn server_message(&mut self, message: ServerMessage) -> Vec<Effect> {
        match message {
            ServerMessage::Ok { id, result } => self.reply(id, result),
            ServerMessage::Error { id, message } => {
                let pending = id.map(|id| self.conn.take_pending(id));
                match pending {
                    Some(Pending::Models) => vec![],
                    // Gone: shown as ended (Enter opens a fresh shell there).
                    Some(Pending::TerminalRead(id)) => {
                        self.terminals.exited(&id);
                        vec![]
                    }
                    Some(Pending::Processes { .. }) => vec![],
                    Some(Pending::TerminalOpen) => {
                        self.terminals.opening = false;
                        self.ui.flash(message, FlashLevel::Error);
                        vec![]
                    }
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
            // A page on the stage changed: drop the cached copy and fetch it again. One you
            // have open in another client keeps its cached copy, and gets it on the way back.
            // The cached copy is stale. Whether it is asked for again is what the stage
            // does next: the page you are reading refetches now, one open on another tab
            // keeps what it has and gets the new text as you return to it.
            ServerMessage::PageChanged { url, .. } => {
                self.page.invalidate(&url);
                vec![]
            }
            // Our stage, or another client's: the same list either way. What this terminal
            // cannot draw stays in `desk` unread and unwritten.
            ServerMessage::DeskChanged {
                session,
                desk,
                by,
                opened,
            } if self.session.current.as_deref() == Some(session.as_str()) => {
                self.desk.set_desk(desk);
                // A tab a terminal can't draw is never shown here, whoever moved the stage.
                let drawable = opened
                    .as_deref()
                    .map(|id| self.desk.can_draw(id))
                    .unwrap_or(true);
                let followed =
                    self.page.follow && opened.as_deref().is_some_and(|id| self.desk.can_draw(id));
                match by {
                    // You moved the stage where you are: that is what it shows. A tab this
                    // terminal can't draw leaves it on what it had.
                    DeskBy::User if drawable => {
                        self.desk.show_desk(&self.visible_ids());
                        vec![]
                    }
                    // The agent opened or pointed at a tab: following it means showing it, and
                    // *showing* it is what marks it read — `desk.focus` is the only thing that
                    // clears `unseen`, and every client reads that badge. Not following leaves
                    // the tab waiting in the bar with its badge, unread by anyone.
                    DeskBy::Agent if followed => self.show_tab_id(&opened.unwrap_or_default()),
                    _ => vec![],
                }
            }
            ServerMessage::TerminalOutput {
                session,
                terminal,
                data,
            } if self.session.current.as_deref() == Some(session.as_str()) => {
                use base64::Engine;
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                    self.terminals.output(&terminal, &bytes);
                }
                vec![]
            }
            ServerMessage::TerminalExited { session, terminal }
                if self.session.current.as_deref() == Some(session.as_str()) =>
            {
                self.terminals.exited(&terminal);
                vec![]
            }
            ServerMessage::ProcessesChanged { session, processes }
                if self.session.current.as_deref() == Some(session.as_str()) =>
            {
                self.terminals.processes = processes;
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
                    self.page.navigate(&url).into_iter().collect()
                } else {
                    vec![]
                }
            }
            other => {
                let current = self.session.apply(&other);
                // The agent may have changed files in the folder: refresh an open changes view.
                let changed = matches!(
                    other,
                    ServerMessage::ToolResult { .. } | ServerMessage::TurnDone { .. }
                );
                if current && changed && self.changes_open() {
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
            (
                Pending::TerminalOpen | Pending::TerminalRead(_),
                CommandResult::Terminal {
                    terminal,
                    scrollback,
                },
            ) => {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(scrollback)
                    .unwrap_or_default();
                self.terminals.open(&terminal, &bytes);
                vec![]
            }
            (Pending::Processes { session }, CommandResult::Processes { processes })
                if self.session.current.as_deref() == Some(session.as_str()) =>
            {
                self.terminals.processes = processes;
                vec![]
            }
            // The stage of the session this TUI shows, from the horse's mouth: this answer is
            // what the tab list is built from after `:attach`, after a resume, after a
            // reconnect. A late answer for a session you have left is dropped.
            (Pending::Desk { session }, CommandResult::Desk { desk })
                if self.session.current.as_deref() == Some(session.as_str()) =>
            {
                self.desk.set_desk(desk);
                // Whatever is still shown here stays; otherwise the desk's own tab, and if
                // this terminal cannot draw that, the last one it can — the same fallback a
                // `by: User` change follows. Nothing, if there is nothing to draw.
                let ids = self.showable_ids();
                self.desk.reconcile(&ids);
                self.page.forget(&self.visible_ids());
                let shown = self.desk.shown().unwrap_or("").to_string();
                self.show_local(&shown, vec![])
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
                    kind: startup.kind,
                    model: None,
                    site: startup.site,
                    harness: None,
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

/// `:mcp`'s line: each server's state, tool count and site; a warning when one failed.
fn mcp_summary(servers: &[composer_protocol::http::McpServerInfo]) -> (String, FlashLevel) {
    if servers.is_empty() {
        return (
            "no MCP servers: add [mcp.<name>] to config.toml (every session), or \
             [servers.<name>] to a site's mcp.toml"
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
            let agents = if s.sites.is_empty() {
                "every session".to_string()
            } else {
                format!("for {}", s.sites.join(", "))
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
    use composer_protocol::{ChatMessage, Desk, DeskBy, DeskEntry, Role, SessionInfo, TurnState};

    use crate::store::types::PageView;

    use super::*;
    use crate::store::types::{ChatItem, PickerKind};

    fn info(id: &str, parent: Option<&str>) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            kind: composer_protocol::RunKind::Agentic,
            site: None,
            folder: None,
            parent: parent.map(Into::into),
            harness: None,
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
                kind: composer_protocol::RunKind::Agentic,
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
    fn site_sends_site_set_and_flashes_the_outcome() {
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
        assert_eq!(state.ui.focus, Pane::Stage);
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
    fn a_session_opens_on_its_last_messages_and_loads_older_ones_at_the_top() {
        let (mut state, _) = connected();
        state.session.upsert(info("s", None));
        let said = |seq, text: &str| ServerMessage::MessageDone {
            session: "s".into(),
            seq,
            message: assistant(text),
        };
        let effects = state.reduce(Action::AttachSession("s".into()));
        // What was open on its desk comes with it: the stage has nothing to list until it does.
        let commands = sent(&effects);
        let (id, _) = commands
            .iter()
            .find(|(_, c)| matches!(c, ClientCommand::SessionHistory { before: None, .. }))
            .expect("its last messages");
        assert!(
            commands.iter().any(|(_, c)| c
                == &ClientCommand::DeskGet {
                    session: "s".into()
                }),
            "and its desk too: {commands:?}"
        );
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

    /// The leg the browser's failure leans on: a copy request is the one the mouse drag uses,
    /// and the app loop answers it with `terminal::copy` (OSC 52 + this machine's clipboard).
    #[test]
    fn a_copy_request_goes_to_the_terminal_clipboard() {
        let (mut state, _) = connected();
        let url = "http://127.0.0.1:7420/w/default/a/app_x9tr/?session=s7";
        assert_eq!(
            state.reduce(Action::CopyText(url.into())),
            [Effect::CopyText(url.into())]
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
            sites: vec!["lab".into()],
        };
        let (text, level) = mcp_summary(&[
            server("github", "ready", None),
            server("broken", "failed", Some("starting `x`: not found")),
            server("docs", "off", None),
        ]);
        assert_eq!(
            text,
            "mcp: github ready, 2 tools, for lab · docs off · broken failed: starting `x`: not found"
        );
        assert_eq!(level, FlashLevel::Warn);
        assert!(mcp_summary(&[]).0.starts_with("no MCP servers"));
    }

    // ---- the stage: the session's desk, as tabs (ticket 0208) ----

    /// What `App::process` does after reducing: whatever the action was, the stage ends up
    /// showing its shown tab. Tests go through this because that is where the app does it.
    fn settle(state: &mut AppState, action: Action) -> Vec<Effect> {
        let mut effects = state.reduce(action);
        effects.extend(state.sync_stage());
        effects
    }

    /// A connected TUI showing a session (a desk belongs to one).
    fn showing_a_session() -> AppState {
        let (mut state, _) = connected();
        state.session.upsert(info("s", None));
        settle(&mut state, Action::AttachSession("s".into()));
        state
    }

    fn entry(id: &str, tab: DeskTab, unseen: bool) -> DeskEntry {
        DeskEntry {
            id: id.into(),
            tab,
            unseen,
        }
    }

    fn page(url: &str) -> DeskEntry {
        let id = page_key(url).expect("a page url");
        entry(&id, DeskTab::Page { url: url.into() }, false)
    }

    /// The desk as the server (or another client) just said it is.
    fn say_desk(
        state: &mut AppState,
        tabs: Vec<DeskEntry>,
        active: Option<&str>,
        by: DeskBy,
    ) -> Vec<Effect> {
        settle(
            state,
            Action::Server(ServerMessage::DeskChanged {
                session: "s".into(),
                desk: Desk {
                    tabs,
                    active: active.map(Into::into),
                },
                by,
                opened: None,
            }),
        )
    }

    fn titles(state: &AppState) -> Vec<String> {
        state.stage_tabs().iter().map(|tab| tab.title()).collect()
    }

    fn shown_title(state: &AppState) -> Option<String> {
        state.shown_tab().map(|tab| tab.title())
    }

    fn fetches(effects: &[Effect]) -> Vec<String> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::FetchPage(url) => Some(url.clone()),
                _ => None,
            })
            .collect()
    }

    /// The page a tab asked for arrives, as `PageLoaded` delivers it.
    fn arrive(state: &mut AppState, url: &str) {
        state.reduce(Action::PageLoaded(PageView {
            url: url.into(),
            title: "t".into(),
            frontmatter: serde_json::Value::Null,
            frontmatter_error: None,
            raw: format!("the body of {url}"),
            backlinks: vec![],
            hash: "h".into(),
        }));
    }

    #[test]
    fn opening_a_page_puts_it_on_the_desk_and_shows_it() {
        let mut state = showing_a_session();
        let effects = settle(&mut state, Action::OpenUrl("site://memory/user".into()));
        assert!(
            sent(&effects).iter().any(|(_, c)| c
                == &ClientCommand::DeskOpen {
                    session: "s".into(),
                    tab: DeskTab::Page {
                        url: "site://memory/user".into()
                    },
                    focus: true,
                }),
            "a desktop window on this session sees the tab: {effects:?}"
        );
        assert_eq!(
            fetches(&effects),
            ["site://memory/user"],
            "and it is fetched to be read"
        );
        // The desk agrees: now it is a tab, with the page's own name as its title.
        say_desk(
            &mut state,
            vec![page("site://memory/user")],
            Some("page:site://memory/user"),
            DeskBy::User,
        );
        assert_eq!(titles(&state), ["memory/user"]);
        assert_eq!(shown_title(&state), Some("memory/user".into()));
    }

    #[test]
    fn two_pages_are_two_tabs_and_the_same_page_twice_is_one() {
        let mut state = showing_a_session();
        // The desk is what says what is open; it derives the id from the page, so a page
        // opened twice is one tab that is focused again — and this is all the TUI lists.
        say_desk(
            &mut state,
            vec![page("site://memory/user"), page("site://plans/x")],
            Some("page:site://plans/x"),
            DeskBy::User,
        );
        assert_eq!(titles(&state), ["memory/user", "plans/x"]);
        assert_eq!(
            shown_title(&state),
            Some("plans/x".into()),
            "the desk's shown tab is the one here"
        );
        // Opening one of them again: the same tab, never a second one — and once its page
        // is here, opening it again asks the server for nothing.
        settle(&mut state, Action::OpenUrl("site://plans/x".into()));
        arrive(&mut state, "site://plans/x");
        let effects = settle(&mut state, Action::OpenUrl("site://plans/x".into()));
        assert_eq!(
            fetches(&effects),
            Vec::<String>::new(),
            "its page is already here: {effects:?}"
        );
        say_desk(
            &mut state,
            vec![page("site://memory/user"), page("site://plans/x")],
            Some("page:site://memory/user"),
            DeskBy::User,
        );
        assert_eq!(state.stage_tabs().len(), 2);
    }

    #[test]
    fn a_tab_shows_its_own_page_and_an_open_one_needs_nothing() {
        let mut state = showing_a_session();
        let effects = say_desk(
            &mut state,
            vec![page("site://a"), page("site://b")],
            Some("page:site://a"),
            DeskBy::User,
        );
        assert_eq!(
            fetches(&effects),
            ["site://a"],
            "showing a tab asks for its page, once"
        );
        arrive(&mut state, "site://a");
        assert_eq!(state.page.current.as_ref().unwrap().url, "site://a");
        // To the other tab: its page, not the one that was on screen.
        let effects = settle(&mut state, Action::CycleTab(1));
        assert_eq!(
            state.page.current, None,
            "not fetched yet, so nothing shown"
        );
        assert_eq!(fetches(&effects), ["site://b"]);
        arrive(&mut state, "site://b");
        // Back to the first: read again from the page it fetched before, no new request.
        let effects = settle(&mut state, Action::CycleTab(-1));
        assert_eq!(state.page.current.as_ref().unwrap().url, "site://a");
        assert_eq!(fetches(&effects), Vec::<String>::new());
        assert_eq!(shown_title(&state), Some("a".into()));
    }

    #[test]
    fn tab_keys_walk_the_tabs_and_wrap_at_the_ends() {
        let mut state = showing_a_session();
        say_desk(
            &mut state,
            vec![page("site://a"), page("site://b")],
            Some("page:site://a"),
            DeskBy::User,
        );
        settle(&mut state, Action::FocusPane(Pane::Chat));
        settle(&mut state, Action::CycleTab(1));
        assert_eq!(shown_title(&state), Some("b".into()));
        assert_eq!(state.ui.focus, Pane::Stage, "you see what you switched to");
        // The command goes to the desk, so the other clients move too.
        let effects = settle(&mut state, Action::CycleTab(1));
        assert!(
            sent(&effects).iter().any(|(_, c)| c
                == &ClientCommand::DeskFocus {
                    session: "s".into(),
                    tab: "page:site://a".into(),
                }),
            "wraps to the first: {effects:?}"
        );
        assert_eq!(shown_title(&state), Some("a".into()));
        settle(&mut state, Action::CycleTab(-1));
        assert_eq!(shown_title(&state), Some("b".into()));
        // A tab by number, as a click on the bar says it.
        settle(&mut state, Action::ShowTab(0));
        assert_eq!(shown_title(&state), Some("a".into()));
    }

    #[test]
    fn closing_a_tab_shows_its_neighbour_and_the_last_one_is_an_empty_stage() {
        let mut state = showing_a_session();
        say_desk(
            &mut state,
            vec![page("site://a"), page("site://b")],
            Some("page:site://a"),
            DeskBy::User,
        );
        let effects = settle(&mut state, Action::CloseTab);
        assert!(
            sent(&effects).iter().any(|(_, c)| c
                == &ClientCommand::DeskClose {
                    session: "s".into(),
                    tab: "page:site://a".into(),
                }),
            "{effects:?}"
        );
        assert_eq!(
            shown_title(&state),
            Some("b".into()),
            "the next one, without waiting for the reply"
        );
        // The desk catching up: one tab left.
        say_desk(
            &mut state,
            vec![page("site://b")],
            Some("page:site://b"),
            DeskBy::User,
        );
        assert_eq!(titles(&state), ["b"]);
        settle(&mut state, Action::CloseTab);
        say_desk(&mut state, vec![], None, DeskBy::User);
        assert_eq!(state.stage_tabs(), vec![]);
        assert_eq!(shown_title(&state), None, "the empty stage, not a panic");
        assert_eq!(state.page.current, None);
        // Keys on an empty stage do nothing at all.
        assert!(settle(&mut state, Action::CycleTab(1)).is_empty());
        let effects = settle(&mut state, Action::CloseTab);
        assert!(
            sent(&effects).is_empty(),
            "nothing to close, nothing to say"
        );
        assert_eq!(
            state.ui.flash.map(|f| f.text).unwrap_or_default(),
            "nothing to close",
            "the key says so rather than doing nothing at all"
        );
    }

    #[test]
    fn kinds_a_terminal_cannot_draw_are_kept_but_neither_shown_nor_written() {
        let mut state = showing_a_session();
        let app = entry("live", DeskTab::LiveView, false);
        say_desk(&mut state, vec![app.clone()], Some("live"), DeskBy::User);
        assert_eq!(
            state.stage_tabs(),
            vec![],
            "the live view isn't ours to draw"
        );
        assert_eq!(shown_title(&state), None);
        assert_eq!(
            state.desk.desk.tabs,
            vec![app],
            "kept exactly as received: the TUI is not its owner"
        );
        // Nothing to walk, nothing to close, and no command back to the desk.
        for action in [Action::CycleTab(1), Action::CycleTab(-1), Action::CloseTab] {
            let effects = settle(&mut state, action.clone());
            assert!(
                sent(&effects).is_empty(),
                "{action:?} must not touch the desk: {effects:?}"
            );
        }
        // And a page next to it is still listed on its own, with the other left alone.
        say_desk(
            &mut state,
            vec![entry("live", DeskTab::LiveView, false), page("site://a")],
            Some("page:site://a"),
            DeskBy::User,
        );
        assert_eq!(titles(&state), ["a"]);
        assert_eq!(state.desk.desk.tabs.len(), 2);
    }

    #[test]
    fn a_page_opened_elsewhere_appears_as_a_tab() {
        let mut state = showing_a_session();
        say_desk(
            &mut state,
            vec![page("site://from/the/desktop")],
            Some("page:site://from/the/desktop"),
            DeskBy::User,
        );
        assert_eq!(titles(&state), ["from/the/desktop"]);
        assert_eq!(
            shown_title(&state),
            Some("from/the/desktop".into()),
            "you moved the stage there, so it shows here too"
        );
        assert_eq!(
            state.page.loading.as_deref(),
            Some("site://from/the/desktop"),
            "and its page is on its way, so there is nothing to read yet"
        );
        arrive(&mut state, "site://from/the/desktop");
        assert_eq!(
            state.page.current.as_ref().unwrap().url,
            "site://from/the/desktop"
        );
    }

    #[test]
    fn the_agent_opens_a_tab_badged_when_you_are_not_following_it() {
        let mut state = showing_a_session();
        say_desk(
            &mut state,
            vec![page("site://a")],
            Some("page:site://a"),
            DeskBy::User,
        );
        settle(&mut state, Action::ToggleFollow);
        assert!(!state.page.follow);
        let mut opened = page("site://agent/page");
        opened.unseen = true;
        let effects = settle(
            &mut state,
            Action::Server(ServerMessage::DeskChanged {
                session: "s".into(),
                desk: Desk {
                    tabs: vec![page("site://a"), opened.clone()],
                    active: Some("page:site://agent/page".into()),
                },
                by: DeskBy::Agent,
                opened: Some("page:site://agent/page".into()),
            }),
        );
        assert_eq!(
            shown_title(&state),
            Some("a".into()),
            "not following: what is on screen stays"
        );
        assert!(sent(&effects).is_empty(), "and nothing is sent back");
        assert_eq!(
            state
                .stage_tabs()
                .iter()
                .map(|tab| tab.unseen())
                .collect::<Vec<_>>(),
            [false, true],
            "it waits in the bar with its badge"
        );
        // Following it means showing it, and asking the desk to focus it: `desk.focus` is the
        // only thing that clears `unseen`, so without it the badge outlives the reading — on
        // the desktop and the phone too, which read the same flag.
        let mut state = showing_a_session();
        let effects = settle(
            &mut state,
            Action::Server(ServerMessage::DeskChanged {
                session: "s".into(),
                desk: Desk {
                    tabs: vec![opened],
                    active: Some("page:site://agent/page".into()),
                },
                by: DeskBy::Agent,
                opened: Some("page:site://agent/page".into()),
            }),
        );
        assert_eq!(shown_title(&state), Some("agent/page".into()));
        assert!(
            sent(&effects).iter().any(|(_, c)| c
                == &ClientCommand::DeskFocus {
                    session: "s".into(),
                    tab: "page:site://agent/page".into(),
                }),
            "reading it marks it read: {effects:?}"
        );
        // The desk agrees; the badge is gone for every client, including this one.
        let read = entry(
            "page:site://agent/page",
            DeskTab::Page {
                url: "site://agent/page".into(),
            },
            false,
        );
        say_desk(
            &mut state,
            vec![read],
            Some("page:site://agent/page"),
            DeskBy::User,
        );
        assert_eq!(
            state
                .stage_tabs()
                .iter()
                .map(|t| t.unseen())
                .collect::<Vec<_>>(),
            [false],
            "nothing badged once it has been read"
        );
    }

    #[test]
    fn the_changes_tab_is_the_sites_changes_and_closes_like_any_other() {
        let mut state = showing_a_session();
        state.session.upsert(SessionInfo {
            site: Some("game".into()),
            ..info("s", None)
        });
        let effects = settle(&mut state, Action::ToggleChanges);
        assert!(
            sent(&effects).iter().any(|(_, c)| c
                == &ClientCommand::DeskOpen {
                    session: "s".into(),
                    tab: DeskTab::Changes {
                        site: "game".into()
                    },
                    focus: true,
                }),
            "the desk gets a changes tab named for the site: {effects:?}"
        );
        say_desk(
            &mut state,
            vec![entry(
                "changes:game",
                DeskTab::Changes {
                    site: "game".into(),
                },
                false,
            )],
            Some("changes:game"),
            DeskBy::User,
        );
        assert_eq!(titles(&state), ["changes · game"]);
        assert!(
            matches!(state.shown_tab(), Some(StageTab::Changes { .. })),
            "so the changes view is the one that gets the keys"
        );
        assert_eq!(state.changes_elsewhere(), None);
        // `gd` again closes it, like any tab.
        let effects = settle(&mut state, Action::ToggleChanges);
        assert!(
            sent(&effects).iter().any(|(_, c)| {
                c == &ClientCommand::DeskClose {
                    session: "s".into(),
                    tab: "changes:game".into(),
                }
            }),
            "{effects:?}"
        );
    }

    /// The stage drawn, not just its state: the bar lists the tabs and marks the one shown,
    /// and a terminal too narrow for two panes shows neither the stage nor its bar.
    #[test]
    fn the_stage_draws_its_tabs_and_a_narrow_terminal_does_not() {
        use crate::components::{Component, Root};
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::layout::Rect;

        let mut state = showing_a_session();
        let draw = |state: &AppState, width: u16| {
            let root = Root::new(vec![]);
            let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
            terminal
                .draw(|frame| root.render(frame, frame.area(), state))
                .unwrap();
            terminal
        };
        let drawn = |state: &AppState, width: u16| -> String {
            crate::snapshot::buffer_to_text(draw(state, width).backend().buffer())
        };
        // Whether a tab is drawn as the one shown: the bar paints it on the accent.
        let marked = |state: &AppState, needle: &str| -> bool {
            let terminal = draw(state, 140);
            let buffer = terminal.backend().buffer();
            let want: Vec<String> = needle.chars().map(|c| c.to_string()).collect();
            (0..buffer.area.height).any(|y| {
                let cells: Vec<(String, bool)> = (0..buffer.area.width)
                    .map(|x| {
                        (
                            buffer[(x, y)].symbol().to_string(),
                            buffer[(x, y)].bg == crate::theme::ACCENT,
                        )
                    })
                    .collect();
                cells.windows(want.len()).any(|window| {
                    window
                        .iter()
                        .map(|cell| cell.0.as_str())
                        .eq(want.iter().map(String::as_str))
                        && window[0].1
                })
            })
        };

        let empty = drawn(&state, 140);
        assert!(
            empty.contains("nothing open"),
            "an empty stage says what fills it:\n{empty}"
        );

        let _ = say_desk(
            &mut state,
            vec![page("site://memory/user"), page("site://plans/x")],
            Some("page:site://plans/x"),
            DeskBy::User,
        );
        let wide = drawn(&state, 140);
        assert!(
            wide.contains("memory/user") && wide.contains("plans/x"),
            "both tabs are on the bar:\n{wide}"
        );
        assert!(
            !wide.contains("nothing open"),
            "the shown tab's page is what is drawn, not the empty stage:\n{wide}"
        );
        assert!(
            marked(&state, "plans/x") && !marked(&state, "memory/user"),
            "the bar marks which of them is shown"
        );

        // Below TWO_PANE_MIN_WIDTH only the focused pane is shown: the stage and its bar go.
        state.ui.focus = Pane::Chat;
        let narrow = drawn(&state, 80);
        assert!(
            !narrow.contains("memory/user"),
            "no stage, no tab bar, at 80 columns:\n{narrow}"
        );

        // A click on a tab picks that tab: the bar answers with its index, so the click and
        // the keys can never disagree about what is open.
        let mut root = Root::new(vec![]);
        let screen = Rect::new(0, 0, 140, 24);
        let click = |root: &mut Root, x: u16| {
            let at = |kind| crossterm::event::MouseEvent {
                kind,
                column: x,
                row: 0,
                modifiers: crossterm::event::KeyModifiers::NONE,
            };
            let left = crossterm::event::MouseButton::Left;
            let down = crossterm::event::MouseEventKind::Down(left);
            let up = crossterm::event::MouseEventKind::Up(left);
            root.handle_mouse(at(down), screen, &state);
            root.handle_mouse(at(up), screen, &state)
        };
        let picked: Vec<Vec<Action>> = (0..140).map(|x| click(&mut root, x)).collect();
        let on_tab: Vec<&Action> = picked
            .iter()
            .filter_map(|actions| match actions[..] {
                [Action::FocusPane(Pane::Stage), Action::ShowTab(_)] => Some(&actions[1]),
                _ => None,
            })
            .collect();
        assert_eq!(
            on_tab.first().map(|a| format!("{a:?}")).as_deref(),
            Some("ShowTab(0)"),
            "the first tab on the bar is the first tab there is; elsewhere the click only \
             focuses: {on_tab:?}"
        );
        assert!(on_tab.len() >= 2, "both tabs are clickable: {on_tab:?}");
    }

    #[test]
    fn a_page_that_failed_is_not_asked_for_on_every_key_but_r_asks_again() {
        let mut state = showing_a_session();
        let _ = say_desk(
            &mut state,
            vec![page("site://gone")],
            Some("page:site://gone"),
            DeskBy::User,
        );
        state.reduce(Action::PageFailed {
            url: "site://gone".into(),
            error: "no such page".into(),
        });
        for _ in 0..3 {
            let effects = settle(&mut state, Action::ToggleThinking);
            assert!(
                fetches(&effects).is_empty(),
                "a key that goes nowhere must not fetch: {effects:?}"
            );
        }
        let effects = settle(&mut state, Action::ReloadPage);
        assert_eq!(
            fetches(&effects),
            ["site://gone"],
            "`r` is still how you ask again"
        );
    }

    #[test]
    fn a_page_that_changed_is_read_again_when_it_is_the_one_being_read() {
        let mut state = showing_a_session();
        let _ = say_desk(
            &mut state,
            vec![page("site://being/read"), page("site://another/tab")],
            Some("page:site://being/read"),
            DeskBy::User,
        );
        arrive(&mut state, "site://being/read");
        arrive(&mut state, "site://another/tab");
        let changed = |url: &str| {
            Action::Server(ServerMessage::PageChanged {
                url: url.into(),
                kind: composer_protocol::PageChangeKind::Modified,
                by: composer_protocol::ChangedBy::Agent,
            })
        };
        // The tab on the screen: the text is stale, so it is asked for again and the old
        // body is not left up while the new one comes.
        let effects = settle(&mut state, changed("site://being/read"));
        assert_eq!(
            fetches(&effects),
            ["site://being/read"],
            "the page you are reading is refetched"
        );
        assert_eq!(state.page.current, None, "not the text it had");
        // A page open on another tab: dropped from the cache, and nothing asked for now.
        let effects = settle(&mut state, changed("site://another/tab"));
        assert_eq!(
            fetches(&effects),
            Vec::<String>::new(),
            "you are not reading it: {effects:?}"
        );
        let effects = settle(&mut state, Action::CycleTab(1));
        assert_eq!(
            fetches(&effects),
            ["site://another/tab"],
            "and it is the new text when you get there"
        );
    }

    #[test]
    fn pointing_at_a_tab_a_terminal_cannot_draw_leaves_the_stage_reading() {
        // The phone put an app on the stage. Two pages are open here; jumping to either of
        // them because the desk's own tab is undrawable would be this terminal deciding for
        // someone else.
        let mut state = showing_a_session();
        say_desk(
            &mut state,
            vec![page("site://reading")],
            Some("page:site://reading"),
            DeskBy::User,
        );
        let app = entry("live", DeskTab::LiveView, true);
        settle(
            &mut state,
            Action::Server(ServerMessage::DeskChanged {
                session: "s".into(),
                desk: Desk {
                    tabs: vec![page("site://reading"), page("site://other"), app],
                    active: Some("live".into()),
                },
                by: DeskBy::User,
                opened: Some("live".into()),
            }),
        );
        assert_eq!(
            shown_title(&state),
            Some("reading".into()),
            "an undrawable tab is nobody to move us"
        );
        assert_eq!(titles(&state), ["reading", "other"]);
    }

    #[test]
    fn the_desk_answer_after_attach_is_what_the_stage_lists() {
        // `desk.get` is the only way a desk reaches this terminal unasked-for: the server does
        // not send one on `session.attach` and does not broadcast on it either. So this answer
        // is the whole of "the tabs you had are back".
        let (mut state, _) = connected();
        state.session.upsert(info("s", None));
        let effects = settle(&mut state, Action::AttachSession("s".into()));
        let asked = sent(&effects);
        let (id, _) = asked
            .iter()
            .find(|(_, c)| matches!(c, ClientCommand::DeskGet { .. }))
            .expect("attaching asks for the session's desk");
        assert!(
            titles(&state).is_empty(),
            "nothing is listed before the answer"
        );
        let unread = entry("live", DeskTab::LiveView, true);
        let effects = settle(
            &mut state,
            Action::Server(ServerMessage::Ok {
                id: *id,
                result: CommandResult::Desk {
                    desk: Desk {
                        tabs: vec![unread, page("site://was/open/there")],
                        active: Some("page:site://was/open/there".into()),
                    },
                },
            }),
        );
        assert_eq!(
            titles(&state),
            ["was/open/there"],
            "the tabs it had are back, and the app among them is not ours"
        );
        assert_eq!(shown_title(&state), Some("was/open/there".into()));
        assert_eq!(
            fetches(&effects),
            ["site://was/open/there"],
            "and it is read, not merely listed"
        );
    }

    #[test]
    fn a_desk_answer_for_a_session_you_have_left_is_dropped() {
        let (mut state, _) = connected();
        state.session.upsert(info("old", None));
        state.session.upsert(info("new", None));
        let effects = settle(&mut state, Action::AttachSession("old".into()));
        let asked = sent(&effects);
        let (stale, _) = asked
            .iter()
            .find(|(_, c)| matches!(c, ClientCommand::DeskGet { .. }))
            .expect("its desk was asked for");
        settle(&mut state, Action::AttachSession("new".into()));
        settle(
            &mut state,
            Action::Server(ServerMessage::Ok {
                id: *stale,
                result: CommandResult::Desk {
                    desk: Desk {
                        tabs: vec![page("site://from/the/old/session")],
                        active: Some("page:site://from/the/old/session".into()),
                    },
                },
            }),
        );
        assert!(
            titles(&state).is_empty(),
            "the stage shows the session you are on, not the one you asked about"
        );
    }

    #[test]
    fn a_desk_answer_shows_what_it_can_when_the_desks_own_tab_is_not_ours() {
        // The desk's active tab is an app the phone opened: this terminal has no way to draw
        // it, and an empty stage while a page sits on the desk is worse than showing that.
        let mut state = showing_a_session();
        let effects = settle(&mut state, Action::AttachSession("s".into()));
        let asked = sent(&effects);
        let (id, _) = asked
            .iter()
            .find(|(_, c)| matches!(c, ClientCommand::DeskGet { session } if session == "s"))
            .expect("its desk was asked for");
        let app = entry("live", DeskTab::LiveView, false);
        settle(
            &mut state,
            Action::Server(ServerMessage::Ok {
                id: *id,
                result: CommandResult::Desk {
                    desk: Desk {
                        tabs: vec![app, page("site://first"), page("site://the/app/is/instead")],
                        active: Some("live".into()),
                    },
                },
            }),
        );
        assert_eq!(titles(&state), ["first", "the/app/is/instead"]);
        assert_eq!(
            shown_title(&state),
            Some("the/app/is/instead".into()),
            "the last tab a terminal can draw, the same fallback a change follows"
        );
    }

    #[test]
    fn another_session_comes_with_its_own_tabs() {
        let mut state = showing_a_session();
        state.session.upsert(info("t", None));
        say_desk(
            &mut state,
            vec![page("site://here")],
            Some("page:site://here"),
            DeskBy::User,
        );
        assert_eq!(titles(&state), ["here"]);
        settle(&mut state, Action::AttachSession("t".into()));
        assert_eq!(
            titles(&state),
            Vec::<String>::new(),
            "what was open belongs to the session you were on"
        );
        assert_eq!(state.page.current, None);
    }

    #[test]
    fn a_terminal_tab_reads_its_terminal_draws_its_output_and_lists_processes() {
        let mut state = showing_a_session();
        let tab = entry("terminal:t1", DeskTab::Terminal { id: "t1".into() }, false);
        let effects = say_desk(&mut state, vec![tab], Some("terminal:t1"), DeskBy::User);
        assert_eq!(state.shown_terminal().as_deref(), Some("t1"));
        assert!(
            sent(&effects).iter().any(|(_, c)| *c
                == ClientCommand::TerminalRead {
                    terminal: "t1".into()
                }),
            "{effects:?}"
        );
        let (id, _) = sent(&effects)
            .into_iter()
            .find(|(_, c)| matches!(c, ClientCommand::TerminalRead { .. }))
            .unwrap();
        settle(
            &mut state,
            Action::Server(ServerMessage::Ok {
                id,
                result: CommandResult::Terminal {
                    terminal: "t1".into(),
                    scrollback: "b2xkCg==".into(), // "old\n"
                },
            }),
        );
        settle(
            &mut state,
            Action::Server(ServerMessage::TerminalOutput {
                session: "s".into(),
                terminal: "t1".into(),
                data: "bmV3".into(), // "new"
            }),
        );
        let text = state.terminals.screen("t1").unwrap().text();
        assert_eq!(text[0], "old");
        assert!(text[1].ends_with("new"), "{text:?}");
        // Focused on the stage, keys belong to the terminal.
        settle(&mut state, Action::FocusPane(Pane::Stage));
        assert!(state.terminal_focused());

        // The session's background processes: listed, the only one stopped by `:stop`.
        settle(
            &mut state,
            Action::Server(ServerMessage::ProcessesChanged {
                session: "s".into(),
                processes: vec![composer_protocol::ProcessInfo {
                    terminal: "t1".into(),
                    command: "npm run dev".into(),
                    cwd: "/p".into(),
                    started: "2026-11-21T12:00:00Z".into(),
                }],
            }),
        );
        assert_eq!(titles(&state), ["$ npm run dev"]);
        let effects = settle(&mut state, Action::StopProcess(None));
        assert!(
            sent(&effects).iter().any(|(_, c)| *c
                == ClientCommand::ProcessStop {
                    terminal: "t1".into()
                }),
            "{effects:?}"
        );
        settle(
            &mut state,
            Action::Server(ServerMessage::TerminalExited {
                session: "s".into(),
                terminal: "t1".into(),
            }),
        );
        assert!(state.terminals.ended.contains("t1"));
    }
}
