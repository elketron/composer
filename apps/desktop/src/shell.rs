//! A workspace's window: the title bar, the chat column (the native conversation of the shown
//! session) and the stage (its desk as tabs).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use base64::Engine;
use composer_client::{Connection, Endpoint, Event, Store};
use composer_core::LinkTarget;
use composer_protocol::http::SiteSummary;
use composer_protocol::{ClientCommand, CommandResult, DeskEntry, DeskTab, ServerMessage};
use gpui::{
    Context, Entity, ExternalPaths, FocusHandle, Focusable, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Subscription, Window, actions, div, prelude::*, px, rgb,
};
use tokio::sync::mpsc;

use crate::browser_tabs::BrowserTabs;
use crate::chat_pane::{ChatEvent, ChatPane};
use crate::chat_width::ChatWidth;
use crate::desktop::{self, Desktop};
use crate::home_view::{HomeEvent, HomeTab, HomeView};
use crate::link_state::LinkState;
use crate::notice::Notice;
use crate::pick_lists;
use crate::picker::{Item, Pick, Picker, PickerEvent};
use crate::project_binding::ProjectBinding;
use crate::stage_tabs;
use crate::term_pane::{FONT, PaneEvent, TermPane};
use crate::terminal_link::TerminalLink;
use crate::theme::*;
use crate::view_urls::{AuthCookie, auth_cookie, origin, paths_text, view_url};
use crate::web_pane::{WebEvent, WebPane};

actions!(
    desktop,
    [
        FocusChat,
        FocusStage,
        Home,
        NextTab,
        PrevTab,
        Palette,
        PickWorkspace,
        PickSession,
        Workspace1,
        Workspace2,
        Workspace3,
        Workspace4,
        Workspace5,
        Workspace6,
        Workspace7,
        Workspace8,
        Workspace9
    ]
);

const FIRST_IDS: u64 = 1;
/// Messages a chat opens with (older ones come as you ask).
const HISTORY: u32 = 60;
/// The agent a window's first session gets, when the workspace has none yet.
const FIRST_AGENT: &str = "default";

pub struct Shell {
    workspace: String,
    /// The workspace's colour (its stripe and name), so which one is in front is unmistakable.
    colour: u32,
    /// The palette or a picker, over the window.
    overlay: Option<Entity<Picker>>,
    endpoint: Endpoint,
    connection: Connection,
    ids: Arc<AtomicU64>,
    store: Store,
    project: ProjectBinding,
    link: LinkState,
    chat: Entity<ChatPane>,
    /// The workspace home, shown instead of the chat and the stage.
    home: Option<Entity<HomeView>>,
    /// Stage terminals, by terminal id.
    panes: HashMap<String, Entity<TermPane>>,
    /// Browser panes of the other tabs; made when a tab is first shown.
    webs: BrowserTabs<Entity<WebPane>>,
    subscriptions: Vec<Subscription>,
    /// The workspace's sites, as last fetched (the palette and the live view's name).
    sites: Vec<SiteSummary>,
    chat_width: ChatWidth,
    dragging: bool,
    /// Show what the agent opens (else it is only marked unseen). The client's choice.
    follow: bool,
    /// The window is in front (as of its last draw): notifications only when it isn't.
    active: bool,
    /// Drawn once (start-up timing).
    drawn: bool,
    visibility_observed: bool,
    window_visible: bool,
    focus: FocusHandle,
}

impl Shell {
    pub fn new(
        workspace: String,
        colour: u32,
        endpoint: Endpoint,
        connection: Connection,
        mut events: mpsc::UnboundedReceiver<Event>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.spawn(async move |this, cx| {
            while let Some(event) = events.recv().await {
                if this
                    .update(cx, |shell, cx| shell.on_event(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let chat = cx.new(ChatPane::new);
        let subscriptions = vec![cx.subscribe(&chat, |shell, _, event: &ChatEvent, cx| {
            shell.on_chat(event.clone(), cx)
        })];
        Self {
            workspace,
            colour,
            overlay: None,
            endpoint,
            connection,
            ids: Arc::new(AtomicU64::new(FIRST_IDS)),
            store: Store::default(),
            project: ProjectBinding::default(),
            link: LinkState::Connecting,
            chat,
            home: None,
            panes: HashMap::new(),
            webs: BrowserTabs::default(),
            subscriptions,
            sites: Vec::new(),
            chat_width: ChatWidth::DEFAULT,
            dragging: false,
            follow: true,
            active: true,
            drawn: false,
            visibility_observed: false,
            window_visible: true,
            focus: cx.focus_handle(),
        }
    }

    fn on_event(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Connected => {
                crate::trace("connected");
                self.link = LinkState::Up;
                self.project.reset();
                self.store.reconnected();
                self.sync_approvals(cx);
                self.sync_home(cx);
                // All the sessions, for the switcher (updates follow as events); the first
                // reply also picks the session to show.
                self.ask(ClientCommand::SessionList, cx);
                self.refresh_sites(cx);
                // Stage panes start over from the server's scrollback; apps get fresh tokens.
                self.panes.clear();
                self.webs.clear();
                if let Some(session) = self.store.attached.clone() {
                    // Back where the chat was: what happened since.
                    let since = Some(self.chat.read(cx).last_seq()).filter(|s| *s > 0);
                    self.ask(
                        ClientCommand::SessionAttach {
                            session: session.clone(),
                            since,
                        },
                        cx,
                    );
                    self.ask(ClientCommand::DeskGet { session }, cx);
                }
            }
            Event::Disconnected(reason) => {
                self.link = LinkState::Down(reason);
            }
            Event::Message(message) => self.on_message(message, cx),
        }
        cx.notify();
    }

    fn on_message(&mut self, message: ServerMessage, cx: &mut Context<Self>) {
        if let ServerMessage::TerminalOutput { terminal, data, .. } = &message {
            self.feed(terminal, data, cx);
            return; // the panes keep the screen; the store's copy isn't needed here
        }
        self.notify_if_away(&message);
        if let ServerMessage::TerminalExited { terminal, .. } = &message
            && let Some(pane) = self.panes.get(terminal)
        {
            pane.update(cx, |pane, cx| pane.set_ended(cx));
        }
        for command in self.store.apply(&message) {
            self.ask(command, cx);
        }
        self.chat.update(cx, |chat, cx| chat.apply(&message, cx));
        if let Some(home) = self.home.clone() {
            home.update(cx, |home, cx| home.apply(&message, cx));
        }
        if matches!(
            message,
            ServerMessage::SessionUpdated { .. }
                | ServerMessage::ApprovalRequest { .. }
                | ServerMessage::ApprovalResolved { .. }
        ) {
            self.sync_home(cx);
        }
        match &message {
            ServerMessage::ApprovalRequest { .. } | ServerMessage::ApprovalResolved { .. } => {
                self.sync_approvals(cx)
            }
            ServerMessage::SessionUpdated { session } if self.is_shown(&session.id) => {
                self.sync_info(cx)
            }
            _ => {}
        }
        if let ServerMessage::DeskChanged {
            session, opened, ..
        } = &message
        {
            self.sync_panes(cx);
            // The agent opened something: a following client shows it.
            if let Some(tab) = opened
                && self.follow
                && self.store.attached.as_ref() == Some(session)
            {
                self.show_tab(tab.clone(), cx);
            }
        }
    }

    fn is_shown(&self, session: &str) -> bool {
        self.store.attached.as_deref() == Some(session)
    }

    /// Shows `session`: the chat opens on its last messages, then attaches since them; the
    /// stage shows its desk.
    fn show_session(&mut self, session: String, cx: &mut Context<Self>) {
        if self.is_shown(&session) {
            return;
        }
        if let Some(previous) = self.store.attached.clone() {
            self.ask(ClientCommand::SessionDetach { session: previous }, cx);
        }
        self.store.attach(&session);
        self.chat.update(cx, |chat, cx| chat.open(&session, cx));
        self.sync_info(cx);
        self.sync_approvals(cx);
        self.sync_panes(cx);
        self.ask(
            ClientCommand::DeskGet {
                session: session.clone(),
            },
            cx,
        );
        let connection = self.connection.clone();
        cx.spawn(async move |this, cx| {
            let history = connection
                .request(ClientCommand::SessionHistory {
                    session: session.clone(),
                    before: None,
                    messages: HISTORY,
                })
                .await;
            let _ = this.update(cx, |shell, cx| {
                if !shell.is_shown(&session) {
                    return; // another one was picked meanwhile
                }
                let events = match history {
                    Ok(CommandResult::History { events, more }) => {
                        shell.chat.update(cx, |chat, cx| {
                            chat.history_loaded(&session, &events, more, cx)
                        });
                        events
                    }
                    Ok(_) => vec![],
                    Err(e) => {
                        shell.link = shell.link.refused(e);
                        vec![]
                    }
                };
                let since = Some(shell.chat.read(cx).last_seq()).filter(|s| *s > 0);
                if events.is_empty() {
                    // Nothing to show yet (or an older server): the attach replays it all.
                    shell
                        .chat
                        .update(cx, |chat, cx| chat.history_loaded(&session, &[], false, cx));
                }
                shell.ask(ClientCommand::SessionAttach { session, since }, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// The first session list: show the latest of the user's sessions, or start one.
    fn pick_first_session(&mut self, cx: &mut Context<Self>) {
        if self.store.attached.is_some() {
            return;
        }
        let latest = self
            .store
            .sessions
            .iter()
            .filter(|s| s.worker.is_none())
            .max_by(|a, b| a.updated.cmp(&b.updated))
            .map(|s| s.id.clone());
        match latest {
            Some(session) => self.show_session(session, cx),
            None => self.new_session(FIRST_AGENT.into(), None, cx),
        }
    }

    fn new_session(&mut self, agent: String, project: Option<String>, cx: &mut Context<Self>) {
        let connection = self.connection.clone();
        cx.spawn(async move |this, cx| {
            let created = connection
                .request(ClientCommand::SessionCreate { agent, project })
                .await;
            let _ = this.update(cx, |shell, cx| match created {
                Ok(CommandResult::Session { session }) => {
                    let id = session.id.clone();
                    shell
                        .store
                        .apply(&ServerMessage::SessionUpdated { session });
                    shell.show_session(id, cx);
                }
                Ok(_) => {}
                Err(e) => {
                    shell.link = shell.link.refused(e);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The shown session's agent, state and site, for the chat.
    fn sync_info(&mut self, cx: &mut Context<Self>) {
        let project = self.shown_info().and_then(|info| info.project.clone());
        for command in self.project.sync(project.as_deref()) {
            self.ask(command, cx);
        }
        let info = self
            .store
            .attached
            .as_ref()
            .and_then(|id| self.store.sessions.iter().find(|s| &s.id == id))
            .map(|s| (s.agent.clone(), s.state, s.site.clone()));
        if let Some((agent, state, site)) = info {
            self.chat
                .update(cx, |chat, cx| chat.set_info(&agent, state, site, cx));
        }
    }

    fn sync_approvals(&mut self, cx: &mut Context<Self>) {
        let waiting: Vec<_> = self
            .store
            .approvals
            .values()
            .filter(|a| self.is_shown(&a.session))
            .cloned()
            .collect();
        self.chat
            .update(cx, |chat, cx| chat.set_approvals(waiting, cx));
    }

    /// Shows the workspace home on `tab` (instead of the chat and the stage).
    fn open_home(&mut self, tab: HomeTab, window: &mut Window, cx: &mut Context<Self>) {
        let home = match self.home.clone() {
            Some(home) => {
                home.update(cx, |home, cx| home.set_tab(tab, cx));
                home
            }
            None => {
                let (workspace, colour) = (self.workspace.clone(), self.colour);
                let (connection, endpoint) = (self.connection.clone(), self.endpoint.clone());
                let home =
                    cx.new(|cx| HomeView::new(tab, workspace, colour, connection, endpoint, cx));
                self.subscriptions.push(cx.subscribe_in(
                    &home,
                    window,
                    |shell, _, event: &HomeEvent, window, cx| {
                        shell.on_home(event.clone(), window, cx)
                    },
                ));
                self.home = Some(home.clone());
                self.sync_home(cx);
                home
            }
        };
        window.focus(&home.read(cx).focus_handle(cx), cx);
        self.sync_web_visibility(cx);
        cx.notify();
    }

    /// Back to the session screen.
    fn close_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.home.take().is_some() {
            window.focus(&self.chat.read(cx).focus_handle(cx), cx);
            self.sync_web_visibility(cx);
            cx.notify();
        }
    }

    fn sync_home(&mut self, cx: &mut Context<Self>) {
        if let Some(home) = self.home.clone() {
            let (sessions, approvals) = (&self.store.sessions, &self.store.approvals);
            home.update(cx, |home, cx| home.set_sessions(sessions, approvals, cx));
        }
    }

    fn on_home(&mut self, event: HomeEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            HomeEvent::Open(session) => {
                self.close_home(window, cx);
                self.show_session(session, cx);
            }
            HomeEvent::Back => self.close_home(window, cx),
            HomeEvent::NewSession(project) => {
                self.close_home(window, cx);
                self.new_session(FIRST_AGENT.into(), project, cx);
            }
            HomeEvent::Answer { request, decision } => {
                self.ask(ClientCommand::ApprovalRespond { request, decision }, cx)
            }
        }
    }

    /// What the chat asks.
    fn on_chat(&mut self, event: ChatEvent, cx: &mut Context<Self>) {
        // A link clicked in the chat: what the window already does with links a pane reports -
        // a page of the web opens as a tab, an outside address goes to the OS.
        if let ChatEvent::Open { target } = event {
            match target {
                LinkTarget::Site(url) => self.open_page(url.to_string(), cx),
                LinkTarget::External(url) => {
                    if let Err(e) = open::that_detached(&url) {
                        self.link = self.link.refused(format!("couldn't open {url}: {e}"));
                        cx.notify();
                    }
                }
                LinkTarget::Invalid(href) => {
                    self.link = self.link.refused(format!("can't open {href}"));
                    cx.notify();
                }
            }
            return;
        }
        let Some(session) = self.store.attached.clone() else {
            self.link = self.link.refused("no session yet".into());
            cx.notify();
            return;
        };
        match event {
            ChatEvent::Send { text, attachments } => self.ask(
                ClientCommand::MessageSend {
                    session,
                    text,
                    pages: vec![],
                    images: vec![],
                    attachments,
                },
                cx,
            ),
            ChatEvent::Answer { request, decision } => {
                self.ask(ClientCommand::ApprovalRespond { request, decision }, cx)
            }
            ChatEvent::Cancel => self.ask(ClientCommand::TurnCancel { session }, cx),
            ChatEvent::Attach => self.attach_file(cx),
            ChatEvent::PasteFiles(paths) => self.upload(paths, cx),
            ChatEvent::PasteImage { name, bytes } => {
                self.upload_one(name, async move { Ok(bytes) }, cx)
            }
            // handled above, before the session is needed
            ChatEvent::Open { .. } => {}
            ChatEvent::LoadOlder => {
                let before = Some(self.chat.read(cx).first_seq());
                let connection = self.connection.clone();
                cx.spawn(async move |this, cx| {
                    let older = connection
                        .request(ClientCommand::SessionHistory {
                            session: session.clone(),
                            before,
                            messages: HISTORY,
                        })
                        .await;
                    let _ = this.update(cx, |shell, cx| {
                        let (events, more) = match older {
                            Ok(CommandResult::History { events, more }) => (events, more),
                            _ => (vec![], false),
                        };
                        shell.chat.update(cx, |chat, cx| {
                            chat.older_loaded(&session, &events, more, cx)
                        });
                    });
                })
                .detach();
            }
        }
    }

    /// Sends `command`; its reply goes into the store and the panes.
    fn ask(&mut self, command: ClientCommand, cx: &mut Context<Self>) {
        let connection = self.connection.clone();
        cx.spawn(async move |this, cx| {
            let answer = connection.request(command).await;
            let _ = this.update(cx, |shell, cx| {
                match answer {
                    Ok(result) => shell.on_result(result, cx),
                    Err(e) => shell.link = shell.link.refused(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn on_result(&mut self, result: CommandResult, cx: &mut Context<Self>) {
        for command in self.store.apply(&ServerMessage::Ok {
            id: 0,
            result: result.clone(),
        }) {
            self.ask(command, cx);
        }
        match result {
            CommandResult::Sessions { .. } => {
                self.pick_first_session(cx);
                self.sync_info(cx);
                self.sync_home(cx);
            }
            CommandResult::Session { session } if self.is_shown(&session.id) => self.sync_info(cx),
            CommandResult::Terminal {
                terminal,
                scrollback,
            } => {
                if !self.panes.contains_key(&terminal) {
                    let pane = self.new_pane(terminal.clone(), cx);
                    self.panes.insert(terminal.clone(), pane);
                }
                // Scrollback: replayed without answering the queries in it.
                if let (Ok(bytes), Some(pane)) = (
                    base64::engine::general_purpose::STANDARD.decode(&scrollback),
                    self.panes.get(&terminal).cloned(),
                ) {
                    pane.update(cx, |pane, cx| pane.replay(&bytes, cx));
                }
            }
            CommandResult::Desk { .. } => self.sync_panes(cx),
            _ => {}
        }
    }

    fn new_pane(&mut self, terminal: String, cx: &mut Context<Self>) -> Entity<TermPane> {
        let link = TerminalLink::new(self.connection.clone(), terminal, self.ids.clone());
        let pane = cx.new(|cx| TermPane::new(link, cx));
        self.subscriptions.push(cx.subscribe(
            &pane,
            |shell, pane, event: &PaneEvent, cx| match event {
                PaneEvent::Reopen => shell.reopen(pane.read(cx).terminal().to_string(), cx),
            },
        ));
        pane
    }

    fn feed(&mut self, terminal: &str, data: &str, cx: &mut Context<Self>) {
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
            return;
        };
        if let Some(pane) = self.panes.get(terminal).cloned() {
            pane.update(cx, |pane, cx| pane.feed(&bytes, cx));
        }
    }

    /// A pane for every terminal on the desk (its scrollback read from the server); none for the
    /// ones gone from it.
    fn sync_panes(&mut self, cx: &mut Context<Self>) {
        let on_desk: Vec<String> = self
            .store
            .desk
            .tabs
            .iter()
            .filter_map(|entry| match &entry.tab {
                DeskTab::Terminal { id } => Some(id.clone()),
                _ => None,
            })
            .collect();
        self.panes.retain(|id, _| on_desk.contains(id));
        let tabs: HashSet<&str> = self.store.desk.tabs.iter().map(|t| t.id.as_str()).collect();
        self.webs.keep(&tabs);
        for id in on_desk {
            if self.panes.contains_key(&id) {
                continue;
            }
            let pane = self.new_pane(id.clone(), cx);
            self.panes.insert(id.clone(), pane.clone());
            self.read_terminal(id, pane, cx);
        }
    }

    /// Its scrollback; a terminal that ended (idle) says so and offers a fresh one.
    fn read_terminal(&mut self, terminal: String, pane: Entity<TermPane>, cx: &mut Context<Self>) {
        let connection = self.connection.clone();
        cx.spawn(async move |this, cx| {
            let answer = connection
                .request(ClientCommand::TerminalRead { terminal })
                .await;
            let _ = this.update(cx, |shell, cx| match answer {
                Ok(result) => shell.on_result(result, cx),
                Err(_) => pane.update(cx, |pane, cx| pane.set_ended(cx)),
            });
        })
        .detach();
    }

    fn reopen(&mut self, terminal: String, cx: &mut Context<Self>) {
        let Some(session) = self.store.attached.clone() else {
            return;
        };
        let (cols, rows) = self
            .panes
            .get(&terminal)
            .map(|p| p.read(cx).grid())
            .unwrap_or((80, 24));
        self.ask(
            ClientCommand::TerminalOpen {
                session,
                terminal: Some(terminal),
                cols: cols.max(10) as u16,
                rows: rows.max(4) as u16,
            },
            cx,
        );
    }

    fn new_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.store.attached.clone() else {
            self.link = self.link.refused("no session yet".into());
            return;
        };
        self.ask(
            ClientCommand::TerminalOpen {
                session,
                terminal: None,
                cols: 100,
                rows: 30,
            },
            cx,
        );
    }

    fn show_tab(&mut self, tab: String, cx: &mut Context<Self>) {
        if let Some(session) = self.store.attached.clone() {
            self.ask(ClientCommand::DeskFocus { session, tab }, cx);
        }
    }

    fn close_tab(&mut self, tab: String, cx: &mut Context<Self>) {
        if let Some(session) = self.store.attached.clone() {
            self.ask(ClientCommand::DeskClose { session, tab }, cx);
        }
    }

    fn shown_entry(&self) -> Option<&DeskEntry> {
        let shown = stage_tabs::shown(&self.store.desk)?;
        self.store.desk.tabs.iter().find(|t| t.id == shown)
    }

    fn shown_pane(&self) -> Option<&Entity<TermPane>> {
        match &self.shown_entry()?.tab {
            DeskTab::Terminal { id } => self.panes.get(id),
            _ => None,
        }
    }

    fn shown_web(&self) -> Option<&Entity<WebPane>> {
        self.webs.get(&self.shown_entry()?.id)
    }

    /// The keyboard follows the stage's shown tab. When the tab changes (ctrl+tab, a pick in the
    /// palette, the agent) while a pane now hidden has the keyboard, the shown pane takes it;
    /// otherwise no pane, and none of the desktop's own keys, work until the pointer moves.
    fn keep_stage_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay.is_some() {
            return;
        }
        let shown_term = self.shown_pane();
        let shown_web = self.shown_web();
        let Some(handle) = shown_term
            .map(|pane| pane.read(cx).focus_handle(cx))
            .or_else(|| shown_web.map(|web| web.read(cx).focus_handle(cx)))
        else {
            return;
        };
        if handle.is_focused(window) {
            return;
        }
        let shown_tab = stage_tabs::shown(&self.store.desk);
        let stray = self
            .panes
            .values()
            .filter(|pane| Some(pane.entity_id()) != shown_term.map(|p| p.entity_id()))
            .map(|pane| pane.read(cx).focus_handle(cx))
            .chain(
                self.webs
                    .hidden(shown_tab)
                    .map(|web| web.read(cx).focus_handle(cx)),
            )
            .any(|hidden| hidden.is_focused(window));
        if !stray {
            return;
        }
        window.focus(&handle, cx);
        if let Some(web) = shown_web.cloned() {
            // CEF has its own focus: without it the page sees no keys.
            web.update(cx, |web, _| web.browser_focus(true));
        }
    }

    fn sync_web_visibility(&self, cx: &mut Context<Self>) {
        let shown = stage_tabs::shown(&self.store.desk);
        let visible = self.window_visible && self.home.is_none();
        for (pane, visible) in self.webs.visibility(shown, visible) {
            pane.update(cx, |pane, cx| pane.set_visible(visible, cx));
        }
    }

    /// The shown tab's browser pane, if it is a browser tab and has none yet: a view's URL right
    /// away, an app's from `app.open`.
    fn load_shown(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.shown_entry().cloned() else {
            return;
        };
        if matches!(entry.tab, DeskTab::Terminal { .. }) || !self.webs.begin(&entry.id) {
            return;
        }
        if let Some(url) = view_url(&self.endpoint, &entry.tab) {
            let live = entry.tab == DeskTab::LiveView;
            // A fresh browser session for the pane's cookie, never the token.
            let session = self.web_session(cx);
            cx.spawn(async move |this, cx| {
                let session = session.await;
                let _ = this.update(cx, |shell, cx| match session {
                    Ok(cookie) => shell.show_web(entry.id, url, live, Some(cookie), cx),
                    Err(e) => {
                        shell.webs.failed(&entry.id);
                        shell.link = shell.link.refused(format!("no browser session: {e}"));
                        cx.notify();
                    }
                });
            })
            .detach();
            return;
        }
        let DeskTab::App { site, project } = entry.tab else {
            return;
        };
        let session = self.store.attached.clone();
        let command = ClientCommand::AppOpen {
            site,
            project,
            session,
        };
        let connection = self.connection.clone();
        cx.spawn(async move |this, cx| {
            let answer = connection.request(command).await;
            let _ = this.update(cx, |shell, cx| match answer {
                // The server knows the session it was opened from (the app is told on load).
                Ok(CommandResult::AppOpened { opened }) => {
                    let url = format!("{}{}", origin(shell.endpoint.base()), opened.url);
                    shell.show_web(entry.id, url, false, None, cx);
                }
                Ok(_) => {}
                Err(e) => {
                    shell.webs.failed(&entry.id);
                    shell.link = shell.link.refused(e);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A new browser session of this desktop's credential (`POST <base>/api/web-session`), as the
    /// panes' cookie.
    fn web_session(
        &self,
        cx: &mut Context<Self>,
    ) -> impl std::future::Future<Output = Result<AuthCookie, String>> + use<> {
        let endpoint = self.endpoint.clone();
        let fetched = cx.global::<Desktop>().spawn(async move {
            let session: composer_protocol::http::WebSession =
                endpoint.post("/api/web-session").await?;
            auth_cookie(&endpoint, &session)
                .ok_or_else(|| format!("{} is no http(s) address", endpoint.base()))
        });
        async move { fetched.await.unwrap_or_else(|e| Err(e.to_string())) }
    }

    /// A browser pane for `tab`. `drive`: the page is the live view, whose Drive mode needs the
    /// pane to pass on modifier keys, repeats and drags that leave it (see `WebPane::set_drive`).
    fn show_web(
        &mut self,
        tab: String,
        url: String,
        drive: bool,
        cookie: Option<AuthCookie>,
        cx: &mut Context<Self>,
    ) {
        let base = self.endpoint.base().to_string();
        let pane = cx.new(|cx| WebPane::new(&url, &base, cookie, cx));
        pane.update(cx, |pane, _| pane.set_drive(drive));
        self.subscriptions.push(cx.subscribe(
            &pane,
            |shell, pane, event: &WebEvent, cx| match event {
                WebEvent::OpenPage(url) => shell.open_page(url.clone(), cx),
                // Expired, or the server restarted: a new session, and the page again (once).
                WebEvent::SessionLost => {
                    let session = shell.web_session(cx);
                    let pane = pane.downgrade();
                    cx.spawn(async move |_, cx| {
                        if let Ok(cookie) = session.await {
                            let _ = pane.update(cx, |pane, _| pane.renew(cookie));
                        }
                    })
                    .detach();
                }
                // Only http(s) and mail links get here (view_urls::external).
                WebEvent::OpenOutside(url) => {
                    if let Err(e) = open::that_detached(url) {
                        shell.link = shell.link.refused(format!("couldn't open {url}: {e}"));
                        cx.notify();
                    }
                }
            },
        ));
        self.webs.arrived(tab, pane);
        self.sync_web_visibility(cx);
        cx.notify();
    }

    fn open_page(&mut self, url: String, cx: &mut Context<Self>) {
        if let Some(session) = self.store.attached.clone() {
            self.ask(
                ClientCommand::DeskOpen {
                    session,
                    tab: DeskTab::Page { url },
                    focus: true,
                },
                cx,
            );
        }
    }

    /// Opens a picker over the window.
    fn show_picker(&mut self, picker: Entity<Picker>, window: &mut Window, cx: &mut Context<Self>) {
        // Focus goes back where it was when the picker closes (else no pane gets keys).
        let back = window
            .focused(cx)
            .unwrap_or_else(|| self.chat.read(cx).focus_handle(cx));
        self.subscriptions.push(cx.subscribe_in(
            &picker,
            window,
            move |shell, _, event: &PickerEvent, window, cx| {
                shell.overlay = None;
                window.focus(&back, cx);
                match event {
                    PickerEvent::Picked(pick) => shell.picked(pick.clone(), window, cx),
                    PickerEvent::Dismissed => {}
                }
                cx.notify();
            },
        ));
        window.focus(&picker.read(cx).focus_handle(cx), cx);
        self.overlay = Some(picker);
        cx.notify();
    }

    fn open_workspaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = pick_lists::workspaces(cx.global::<Desktop>().workspaces(), &self.workspace);
        let picker = cx.new(|cx| Picker::new("workspace", items, cx));
        self.show_picker(picker, window, cx);
    }

    fn open_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = pick_lists::sessions(
            &self.store.sessions,
            self.store.attached.as_deref(),
            &self.store.approvals,
        );
        let mut items = items;
        items.push(Item {
            label: "All sessions…".into(),
            detail: "ctrl+shift+h".into(),
            note: String::new(),
            group: "workspace",
            pick: Pick::Home,
        });
        let picker = cx.new(|cx| Picker::new("session", items, cx));
        self.show_picker(picker, window, cx);
    }

    fn shown_info(&self) -> Option<&composer_protocol::SessionInfo> {
        let id = self.store.attached.as_ref()?;
        self.store.sessions.iter().find(|s| &s.id == id)
    }

    /// The projects to put the shown session in (fetched as it opens).
    fn open_projects_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.shown_info().and_then(|s| s.project.clone());
        let picker = cx.new(|cx| Picker::new("project", vec![], cx));
        let endpoint = self.endpoint.clone();
        let fetched = cx.global::<Desktop>().spawn(async move {
            endpoint
                .get::<Vec<composer_protocol::http::ProjectSummary>>("/api/projects")
                .await
                .unwrap_or_default()
        });
        let weak = picker.downgrade();
        cx.spawn(async move |_, cx| {
            if let Ok(projects) = fetched.await {
                let items = pick_lists::projects(&projects, current.as_deref());
                let _ = weak.update(cx, |picker, cx| picker.extend(items, cx));
            }
        })
        .detach();
        self.show_picker(picker, window, cx);
    }

    /// The provider's models for the shown session.
    fn open_models_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.shown_info().and_then(|s| s.model.clone());
        let picker = cx.new(|cx| Picker::new("model", vec![], cx));
        let connection = self.connection.clone();
        let weak = picker.downgrade();
        cx.spawn(
            async move |this, cx| match connection.request(ClientCommand::ModelList).await {
                Ok(CommandResult::Models {
                    default_model,
                    models,
                }) => {
                    let items = pick_lists::models(&models, &default_model, current.as_deref());
                    let _ = weak.update(cx, |picker, cx| picker.extend(items, cx));
                }
                Ok(_) => {}
                Err(e) => {
                    let _ = this.update(cx, |shell, cx| {
                        shell.link = shell.link.refused(e);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
        self.show_picker(picker, window, cx);
    }

    /// Commands, then sessions; the workspace's sites and Home apps join when fetched.
    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut items = pick_lists::commands(self.follow);
        items.extend(pick_lists::leave_site(self.session_site()));
        items.extend(pick_lists::sessions(
            &self.store.sessions,
            self.store.attached.as_deref(),
            &self.store.approvals,
        ));
        let picker = cx.new(|cx| Picker::new("palette", items, cx));
        let endpoint = self.endpoint.clone();
        let fetched = cx.global::<Desktop>().spawn(async move {
            let sites: Vec<composer_protocol::http::SiteSummary> =
                endpoint.get("/api/sites").await.unwrap_or_default();
            let apps: Option<composer_protocol::http::AppList> =
                endpoint.get("/api/apps").await.ok();
            let launchers: Vec<serde_json::Value> =
                endpoint.get("/api/launcher").await.unwrap_or_default();
            (sites, apps, launchers)
        });
        let weak = picker.downgrade();
        cx.spawn(async move |this, cx| {
            let Ok((sites, apps, launchers)) = fetched.await else {
                return;
            };
            // The current site may have changed while the catalog was fetched.
            let site_items = this
                .update(cx, |shell, _| {
                    shell.sites = sites.clone();
                    pick_lists::sites(&sites, shell.session_site())
                })
                .unwrap_or_default();
            let mut more: Vec<Item> = apps
                .map(|list| list.apps)
                .unwrap_or_default()
                .into_iter()
                .map(|app| Item {
                    label: format!("app: {}", app.label),
                    detail: app.site.clone(),
                    note: String::new(),
                    group: "stage",
                    pick: Pick::App(app.site),
                })
                .collect();
            more.extend(launchers.iter().filter_map(|l| {
                let name = l["name"].as_str()?.to_string();
                Some(Item {
                    label: format!("launch: {name}"),
                    detail: l["command"].as_str().unwrap_or_default().to_string(),
                    note: String::new(),
                    group: "launch",
                    pick: Pick::Launch(name),
                })
            }));
            more.extend(site_items);
            more.extend(sites.into_iter().map(|site| Item {
                label: format!("page: {}", site.title),
                detail: format!("site://{}", site.name),
                note: String::new(),
                group: "pages",
                pick: Pick::Page(format!("site://{}", site.name)),
            }));
            let _ = weak.update(cx, |picker, cx| picker.extend(more, cx));
        })
        .detach();
        self.show_picker(picker, window, cx);
    }

    fn picked(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        match pick {
            Pick::Workspace(name) => cx.defer(move |cx| desktop::open(&name, cx)),
            Pick::Session(session) => self.show_session(session, cx),
            Pick::NewSession => {
                let shown = self
                    .store
                    .attached
                    .as_ref()
                    .and_then(|id| self.store.sessions.iter().find(|s| &s.id == id));
                let agent = shown.map_or(FIRST_AGENT.to_string(), |s| s.agent.clone());
                let project = shown.and_then(|s| s.project.clone());
                self.new_session(agent, project, cx);
            }
            Pick::Page(url) => self.open_page(url, cx),
            Pick::App(site) => {
                if let Some(session) = self.store.attached.clone() {
                    let tab = DeskTab::App {
                        site,
                        project: None,
                    };
                    self.ask(
                        ClientCommand::DeskOpen {
                            session,
                            tab,
                            focus: true,
                        },
                        cx,
                    );
                }
            }
            Pick::Site(site) => self.switch_site(Some(site), cx),
            Pick::LeaveSite => self.switch_site(None, cx),
            Pick::NewTerminal => self.new_terminal(cx),
            Pick::Launch(name) => self.launch(name, cx),
            Pick::AttachFile => self.attach_file(cx),
            Pick::ToggleFollow => self.follow = !self.follow,
            Pick::Workspaces => self.open_workspaces(window, cx),
            Pick::Sessions => self.open_sessions(window, cx),
            Pick::ProjectMenu => self.open_projects_menu(window, cx),
            Pick::ModelMenu => self.open_models_menu(window, cx),
            Pick::Compact => {
                if let Some(session) = self.store.attached.clone() {
                    self.ask(ClientCommand::SessionCompact { session }, cx);
                }
            }
            Pick::SetProject(path) => {
                if let Some(session) = self.store.attached.clone() {
                    self.ask(ClientCommand::ProjectSet { session, path }, cx);
                }
            }
            Pick::SetModel(model) => {
                if let Some(session) = self.store.attached.clone() {
                    self.ask(ClientCommand::ModelSet { session, model }, cx);
                }
            }
            Pick::Home => self.open_home(HomeTab::Sessions, window, cx),
            Pick::Projects => self.open_home(HomeTab::Projects, window, cx),
        }
    }

    /// The site the shown session is in.
    fn session_site(&self) -> Option<&str> {
        let attached = self.store.attached.as_deref()?;
        self.store
            .sessions
            .iter()
            .find(|s| s.id == attached)?
            .site
            .as_deref()
    }

    /// The sites listing, for the live view's name and the palette.
    fn refresh_sites(&mut self, cx: &mut Context<Self>) {
        let endpoint = self.endpoint.clone();
        let fetched = cx.global::<Desktop>().spawn(async move {
            endpoint
                .get::<Vec<SiteSummary>>("/api/sites")
                .await
                .unwrap_or_default()
        });
        cx.spawn(async move |this, cx| {
            if let Ok(sites) = fetched.await {
                let _ = this.update(cx, |shell, cx| {
                    shell.sites = sites;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// `site.set` for the shown session: enters `site` (its application starts and
    /// its live view opens on the stage), or leaves the current one. What the server answered,
    /// or why not, goes to a notice.
    fn switch_site(&mut self, site: Option<String>, cx: &mut Context<Self>) {
        let Some(session) = self.store.attached.clone() else {
            self.link = self.link.refused("no session yet".into());
            return;
        };
        let connection = self.connection.clone();
        cx.spawn(async move |this, cx| {
            let answer = connection
                .request(ClientCommand::SiteSet { session, site })
                .await;
            let reply = match answer {
                Ok(CommandResult::Note { text }) => Ok(text),
                Ok(_) => Ok("done".to_string()),
                Err(e) => Err(e),
            };
            let _ = this.update(cx, |shell, cx| {
                if let Err(why) = &reply {
                    shell.link = shell.link.refused(why.clone());
                }
                Notice::site(reply).show();
                cx.notify();
            });
        })
        .detach();
    }

    /// Runs a `[[launcher]]` entry.
    fn launch(&mut self, name: String, cx: &mut Context<Self>) {
        let endpoint = self.endpoint.clone();
        let path = format!("/api/launcher/{name}");
        let done = cx
            .global::<Desktop>()
            .spawn(async move { endpoint.post::<serde_json::Value>(&path).await });
        cx.spawn(async move |this, cx| {
            let answer = done.await.unwrap_or_else(|e| Err(e.to_string()));
            if let Err(e) = answer {
                let _ = this.update(cx, |shell, cx| {
                    shell.link = shell.link.refused(format!("launch {name}: {e}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Files from the desktop's file picker: uploaded to the session, sent with the next message.
    fn attach_file(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let _ = this.update(cx, |shell, cx| shell.upload(paths, cx));
        })
        .detach();
    }

    /// Uploads files to the shown session's attachments; the chat shows them as they go.
    fn upload(&mut self, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        for path in paths {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".into());
            let read = async move { tokio::fs::read(&path).await.map_err(|e| e.to_string()) };
            self.upload_one(name, read, cx);
        }
    }

    /// Uploads one file (its bytes as `read` gives them); the chat shows it as it goes.
    fn upload_one(
        &mut self,
        name: String,
        read: impl Future<Output = Result<Vec<u8>, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.store.attached.clone() else {
            return;
        };
        let upload = self
            .chat
            .update(cx, |chat, cx| chat.uploading(&session, name.clone(), cx));
        let endpoint = self.endpoint.clone();
        let owner = session.clone();
        let sent = cx.global::<Desktop>().spawn(async move {
            let bytes = read.await?;
            endpoint.upload(&session, name, bytes).await
        });
        cx.spawn(async move |this, cx| {
            let result = sent.await.unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |shell, cx| {
                shell
                    .chat
                    .update(cx, |chat, cx| chat.uploaded(&owner, upload, result, cx))
            });
        })
        .detach();
    }

    /// A finished turn or an approval needed, while the window is behind: a notification.
    fn notify_if_away(&self, message: &ServerMessage) {
        if !self.active
            && let Some(notice) =
                Notice::of(message, self.store.attached.as_deref(), &self.workspace)
        {
            notice.show();
        }
    }

    /// The shown session for the title bar: its agent and id, and its project.
    fn title_parts(&self) -> (String, String, String) {
        let session = self
            .store
            .attached
            .as_ref()
            .and_then(|id| self.store.sessions.iter().find(|s| &s.id == id));
        let agent = session
            .map(|s| s.agent.clone())
            .unwrap_or_else(|| "no session".into());
        let id = session.map(|s| format!("#{}", s.id)).unwrap_or_default();
        let project = session
            .and_then(|s| s.project.as_deref())
            .map(|p| pick_lists::project_name(p).to_string())
            .unwrap_or_else(|| "no project".into());
        (agent, id, project)
    }
}

impl Focusable for Shell {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.window_visible = window.is_visible();
        if !self.visibility_observed {
            self.visibility_observed = true;
            self.subscriptions.push(cx.observe_window_visibility(
                window,
                |shell, visibility, _, cx| {
                    shell.window_visible = visibility.is_visible();
                    shell.sync_web_visibility(cx);
                    cx.notify();
                },
            ));
        }
        if !self.drawn {
            self.drawn = true;
            crate::trace("first draw");
            // Keys go to the chat from the start.
            window.focus(&self.chat.read(cx).focus_handle(cx), cx);
        }
        let total = f32::from(window.viewport_size().width);
        self.chat_width = self.chat_width.within(total);
        self.active = window.is_window_active();
        self.load_shown(cx);
        self.sync_web_visibility(cx);
        self.keep_stage_focus(window, cx);
        let shown = stage_tabs::shown(&self.store.desk).map(str::to_string);
        let live_app = stage_tabs::live_app(self.session_site(), &self.sites).map(str::to_string);
        let tabs = self.store.desk.tabs.iter().map(|entry| {
            let id = entry.id.clone();
            let close = entry.id.clone();
            let active = shown.as_deref() == Some(entry.id.as_str());
            div()
                .id(gpui::SharedString::from(format!("tab-{}", entry.id)))
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(9.))
                .px(px(18.))
                .border_r_1()
                .border_t_2()
                .border_color(rgb(BORDER))
                .text_size(px(TEXT_SIZE))
                .when(active, |d| {
                    d.bg(rgb(BG_DEEP))
                        .text_color(rgb(FG_BRIGHT))
                        .border_t_2()
                        .border_color(rgb(ACCENT))
                })
                .when(!active, |d| {
                    d.text_color(rgb(FG_DIM))
                        .hover(|d| d.text_color(rgb(FG_BRIGHT)))
                })
                .on_click(cx.listener(move |shell, _, _, cx| shell.show_tab(id.clone(), cx)))
                .when(entry.unseen, |d| {
                    d.child(div().size(px(6.)).rounded(px(R_DOT)).bg(rgb(ACCENT_SOFT)))
                })
                .child(stage_tabs::label(&entry.tab, live_app.as_deref()))
                .child(
                    div()
                        .id(gpui::SharedString::from(format!("close-{}", entry.id)))
                        .text_color(rgb(FG_FAINT))
                        .hover(|d| d.text_color(rgb(FG_BRIGHT)))
                        .child("×")
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            cx.stop_propagation();
                            shell.close_tab(close.clone(), cx)
                        })),
                )
        });
        let follow_toggle = div()
            .id("follow")
            .text_color(rgb(FG_DIM))
            .hover(|d| d.text_color(rgb(FG_BRIGHT)))
            .child(if self.follow {
                "following agent"
            } else {
                "not following"
            })
            .on_click(cx.listener(|shell, _, _, cx| {
                shell.follow = !shell.follow;
                cx.notify();
            }));
        let stage_body = match (self.shown_pane(), self.shown_web(), shown.is_some()) {
            (Some(pane), _, _) => pane.clone().into_any_element(),
            (None, Some(web), _) => web.clone().into_any_element(),
            (None, None, true) => placeholder("Opening…").into_any_element(),
            (None, None, false) => placeholder(
                "Nothing on the stage. The agent can put apps and pages here; + opens a terminal.",
            )
            .into_any_element(),
        };
        let chat = self.chat.clone().into_any_element();
        let (agent, session_id, project) = self.title_parts();
        let in_home = self.home.is_some();
        let (connection, connection_colour) = match &self.link {
            LinkState::Up => ("connected".to_string(), OK),
            LinkState::Connecting => ("connecting…".to_string(), FG_FAINT),
            other => (other.suffix().trim().to_string(), WARN),
        };
        let waiting = self.store.approvals.len();
        // A key hint: the key in a small box, then what it does.
        let hint = |key: &'static str, what: &'static str| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .px(px(4.))
                        .border_1()
                        .border_color(rgb(BORDER_2))
                        .rounded(px(R_SM))
                        .text_color(rgb(FG_DIM))
                        .child(key),
                )
                .child(what)
        };
        // A title-bar button: a label and its key.
        let button = |id: &'static str, label: &'static str, key: &'static str| {
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .gap(px(9.))
                .px(px(9.))
                .py(px(4.))
                .border_1()
                .border_color(rgb(BORDER_2))
                .rounded(px(R_CONTROL))
                .text_color(rgb(FG_DIM))
                .hover(|d| d.bg(rgb(SURFACE_2)).text_color(rgb(FG_BRIGHT)))
                .child(label)
                .child(
                    div()
                        .px(px(4.))
                        .border_1()
                        .border_color(rgb(BORDER_2))
                        .rounded(px(R_SM))
                        .text_size(px(TEXT_SMALL))
                        .child(key),
                )
        };
        let chat_width = self.chat_width.get();
        // The palette and pickers float over the chat, not the stage: 54px in at a comfortable
        // width, nearer the edges when the chat is narrow.
        let inset = if chat_width > 520. { 54. } else { 14. };

        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(FG))
            .font_family(FONT)
            .text_size(px(TEXT_SIZE))
            .key_context("shell")
            .track_focus(&self.focus)
            .on_action(cx.listener(|shell, _: &Palette, window, cx| shell.open_palette(window, cx)))
            .on_action(
                cx.listener(|shell, _: &PickWorkspace, window, cx| {
                    shell.open_workspaces(window, cx)
                }),
            )
            .on_action(
                cx.listener(|shell, _: &PickSession, window, cx| shell.open_sessions(window, cx)),
            )
            .on_action(cx.listener(|_, _: &Workspace1, _, cx| jump(0, cx)))
            .on_action(cx.listener(|_, _: &Workspace2, _, cx| jump(1, cx)))
            .on_action(cx.listener(|_, _: &Workspace3, _, cx| jump(2, cx)))
            .on_action(cx.listener(|_, _: &Workspace4, _, cx| jump(3, cx)))
            .on_action(cx.listener(|_, _: &Workspace5, _, cx| jump(4, cx)))
            .on_action(cx.listener(|_, _: &Workspace6, _, cx| jump(5, cx)))
            .on_action(cx.listener(|_, _: &Workspace7, _, cx| jump(6, cx)))
            .on_action(cx.listener(|_, _: &Workspace8, _, cx| jump(7, cx)))
            .on_action(cx.listener(|_, _: &Workspace9, _, cx| jump(8, cx)))
            .on_action(cx.listener(|shell, _: &Home, window, cx| {
                shell.open_home(HomeTab::Sessions, window, cx)
            }))
            .on_action(cx.listener(|shell, _: &FocusChat, window, cx| {
                shell.close_home(window, cx);
                window.focus(&shell.chat.read(cx).focus_handle(cx), cx);
            }))
            .on_action(cx.listener(|shell, _: &FocusStage, window, cx| {
                if let Some(pane) = shell.shown_pane() {
                    window.focus(&pane.read(cx).focus_handle(cx), cx);
                } else if let Some(web) = shell.shown_web().cloned() {
                    window.focus(&web.read(cx).focus_handle(cx), cx);
                    web.update(cx, |web, _| web.browser_focus(true));
                }
            }))
            .on_action(cx.listener(|shell, _: &NextTab, _, cx| {
                if let Some(tab) = stage_tabs::cycled(&shell.store.desk, 1).map(str::to_string) {
                    shell.show_tab(tab, cx);
                }
            }))
            .on_action(cx.listener(|shell, _: &PrevTab, _, cx| {
                if let Some(tab) = stage_tabs::cycled(&shell.store.desk, -1).map(str::to_string) {
                    shell.show_tab(tab, cx);
                }
            }))
            .on_mouse_move(cx.listener(move |shell, ev: &MouseMoveEvent, _, cx| {
                if shell.dragging {
                    shell.chat_width = ChatWidth::dragged_to(f32::from(ev.position.x), total);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|shell, _: &MouseUpEvent, _, cx| {
                    shell.dragging = false;
                    cx.notify();
                }),
            )
            // The title bar: the workspace (its colour on the dot: which one, at a glance), the
            // shown session, the link and follow; Sessions and Commands at the right.
            .child(
                div()
                    .h(px(44.))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(14.))
                    .px(px(12.))
                    .bg(rgb(PANEL))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(TEXT_SIZE))
                    .child(div().size(px(8.)).rounded(px(R_DOT)).bg(rgb(self.colour)))
                    .child(
                        // The workspace's name opens its home.
                        div()
                            .id("workspace-name")
                            .text_color(rgb(FG_BRIGHT))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|d| d.text_color(rgb(ACCENT_SOFT)))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.open_home(HomeTab::Sessions, window, cx)
                            }))
                            .child(self.workspace.clone()),
                    )
                    .child(div().text_color(rgb(FG_FAINT)).child("/"))
                    .when(in_home, |d| {
                        d.child(div().text_color(rgb(FG)).child("home"))
                    })
                    .when(!in_home, |d| {
                        d.child(div().text_color(rgb(FG)).child(agent.clone()))
                            .child(div().text_color(rgb(FG_FAINT)).child(session_id.clone()))
                            .child(div().text_color(rgb(FG_FAINT)).child(project))
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(7.))
                            .text_color(rgb(FG_DIM))
                            .child(
                                div()
                                    .size(px(6.))
                                    .rounded(px(R_DOT))
                                    .bg(rgb(connection_colour)),
                            )
                            .child(connection)
                            .child("·")
                            .child(follow_toggle),
                    )
                    .child(div().flex_1())
                    .when(in_home && self.store.attached.is_some(), |d| {
                        d.child(
                            button("back-button", "↩ back to the session", "Esc").on_click(
                                cx.listener(|shell, _, window, cx| shell.close_home(window, cx)),
                            ),
                        )
                    })
                    .when(!in_home, |d| {
                        d.child(button("sessions-button", "Sessions", "Ctrl ⇧ S").on_click(
                            cx.listener(|shell, _, window, cx| shell.open_sessions(window, cx)),
                        ))
                    })
                    .child(button("commands-button", "Commands", "Ctrl ⇧ P").on_click(
                        cx.listener(|shell, _, window, cx| shell.open_palette(window, cx)),
                    )),
            )
            .children(
                self.home
                    .clone()
                    .map(|home| div().flex_1().min_h(px(0.)).child(home)),
            )
            .when(!in_home, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_hidden()
                        .flex()
                        .flex_row()
                        .child(
                            div()
                                .relative()
                                .flex_none()
                                .w(px(chat_width))
                                .h_full()
                                .min_h(px(0.))
                                .overflow_hidden()
                                // Files dropped on the chat: attached to the next message.
                                .on_drop(cx.listener(|shell, paths: &ExternalPaths, _, cx| {
                                    shell.upload(paths.paths().to_vec(), cx);
                                }))
                                // The panel follows the pointer: hover the chat, type into the chat.
                                .on_mouse_move(cx.listener(|shell, _, window, cx| {
                                    if shell.overlay.is_some() {
                                        return;
                                    }
                                    let handle = shell.chat.read(cx).focus_handle(cx);
                                    if !handle.is_focused(window) {
                                        window.focus(&handle, cx);
                                    }
                                }))
                                .child(chat),
                        )
                        .child(
                            // The edge between chat and stage: a hairline, a few pixels to grab.
                            div()
                                .id("chat-edge")
                                .flex_none()
                                .w(px(5.))
                                .h_full()
                                .border_l_1()
                                .border_color(rgb(BORDER_2))
                                .bg(rgb(BG_DEEP))
                                .cursor_col_resize()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|shell, _: &MouseDownEvent, _, cx| {
                                        shell.dragging = true;
                                        cx.notify();
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .h_full()
                                .min_h(px(0.))
                                .flex()
                                .flex_col()
                                .bg(rgb(BG_DEEP))
                                .child(
                                    div()
                                        .id("tab-strip")
                                        .h(px(41.))
                                        .flex()
                                        .flex_row()
                                        .flex_none()
                                        .overflow_x_scroll()
                                        .bg(rgb(PANEL))
                                        .border_b_1()
                                        .border_color(rgb(BORDER))
                                        .children(tabs)
                                        .child(
                                            div()
                                                .id("new-terminal")
                                                .flex()
                                                .items_center()
                                                .px(px(18.))
                                                .text_size(px(TEXT_SIZE))
                                                .text_color(rgb(FG_DIM))
                                                .hover(|d| d.text_color(rgb(FG_BRIGHT)))
                                                .child("+")
                                                .on_click(cx.listener(|shell, _, _, cx| {
                                                    shell.new_terminal(cx)
                                                })),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h(px(0.))
                                        .overflow_hidden()
                                        // Files dropped on a terminal: their quoted paths.
                                        .on_drop(cx.listener(
                                            |shell, paths: &ExternalPaths, _, cx| {
                                                if let Some(pane) = shell.shown_pane().cloned() {
                                                    let text = paths_text(
                                                        paths.paths().iter().map(|p| p.as_path()),
                                                    );
                                                    pane.update(cx, |pane, cx| {
                                                        pane.paste(&text, cx)
                                                    });
                                                }
                                            },
                                        ))
                                        // The panel follows the pointer: hover the stage, type into it.
                                        .on_mouse_move(cx.listener(|shell, _, window, cx| {
                                            if shell.overlay.is_some() {
                                                return;
                                            }
                                            if let Some(pane) = shell.shown_pane() {
                                                let handle = pane.read(cx).focus_handle(cx);
                                                if !handle.is_focused(window) {
                                                    window.focus(&handle, cx);
                                                }
                                            } else if let Some(web) = shell.shown_web().cloned() {
                                                let handle = web.read(cx).focus_handle(cx);
                                                if !handle.is_focused(window) {
                                                    window.focus(&handle, cx);
                                                    // CEF has its own focus: without it the page sees no keys.
                                                    web.update(cx, |web, _| {
                                                        web.browser_focus(true)
                                                    });
                                                }
                                            }
                                        }))
                                        .child(stage_body),
                                ),
                        ),
                )
            })
            // The status bar: what is live, what waits for you, workers; the keys at the right.
            .child(
                div()
                    .h(px(26.))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(16.))
                    .px(px(12.))
                    .bg(rgb(PANEL))
                    .border_t_1()
                    .border_color(rgb(BORDER_2))
                    .text_size(px(TEXT_SMALL))
                    .text_color(rgb(FG_FAINT))
                    .when_some(live_app.clone(), |d, app| {
                        d.child(div().text_color(rgb(OK)).child(format!("● live · {app}")))
                    })
                    .when(waiting > 0, |d| {
                        d.child(
                            div()
                                .text_color(rgb(WARN))
                                .child(format!("! {waiting} waiting")),
                        )
                    })
                    .child(workers_label(cx.global::<Desktop>().workers()))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("status-palette")
                            .hover(|d| d.text_color(rgb(FG_BRIGHT)))
                            .child(hint("Ctrl ⇧ P", "palette"))
                            .on_click(
                                cx.listener(|shell, _, window, cx| shell.open_palette(window, cx)),
                            ),
                    )
                    .child(
                        div()
                            .id("status-sessions")
                            .hover(|d| d.text_color(rgb(FG_BRIGHT)))
                            .child(hint("Ctrl ⇧ S", "sessions"))
                            .on_click(
                                cx.listener(|shell, _, window, cx| shell.open_sessions(window, cx)),
                            ),
                    ),
            )
            // The palette or a picker, over the chat (painted last, above the panes): the chat
            // dims under it, the stage stays as it is.
            .children(self.overlay.clone().map(|picker| {
                div()
                    .absolute()
                    .top(px(44.))
                    .bottom(px(26.))
                    .left_0()
                    .w(px(chat_width))
                    .child(
                        // The scrim takes the clicks (none reach the chat under it); a click
                        // on it closes the picker.
                        div()
                            .id("scrim")
                            .absolute()
                            .inset_0()
                            .occlude()
                            .bg(gpui::rgba((BG_DEEP << 8) | (SCRIM_ALPHA * 255.) as u32))
                            .on_click({
                                let picker = picker.clone();
                                move |_, _, cx| {
                                    picker.update(cx, |_, cx| cx.emit(PickerEvent::Dismissed))
                                }
                            }),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(72.))
                            .left(px(inset))
                            .right(px(inset))
                            .child(picker),
                    )
            }))
    }
}

/// Shows the `n`th workspace's window (`ctrl+alt+1..9`).
fn jump(n: usize, cx: &mut gpui::App) {
    if let Some(name) = cx.global::<Desktop>().workspaces().get(n).cloned() {
        cx.defer(move |cx| desktop::open(&name, cx));
    }
}

fn workers_label(running: u64) -> String {
    match running {
        0 => "no workers running".into(),
        1 => "1 worker running".into(),
        n => format!("{n} workers running"),
    }
}

fn placeholder(text: &'static str) -> impl IntoElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(TEXT_SIZE))
        .bg(rgb(BG_DEEP))
        .text_color(rgb(FG_DIM))
        .child(text)
}
