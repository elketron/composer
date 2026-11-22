//! The home's window: the title bar, the chat column (the native conversation of the shown
//! session) and the stage (its desk as tabs).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use base64::Engine;
use composer_client::{Connection, Endpoint, Event, Store};
use composer_core::LinkTarget;
use composer_protocol::{
    ClientCommand, CommandResult, DeskEntry, DeskTab, ProcessInfo, RunKind, ServerMessage,
};
use gpui::{
    Context, Entity, ExternalPaths, FocusHandle, Focusable, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Subscription, Window, actions, div, prelude::*, px, rgb,
};
use tokio::sync::mpsc;

use crate::browser_tabs::BrowserTabs;
use crate::chat_pane::{ChatEvent, ChatPane};
use crate::chat_width::ChatWidth;
use crate::desktop::Desktop;
use crate::focus_rule::{self, Holder, HoverFocus, Scene, Target};
use crate::home_view::{HomeEvent, HomeView};
use crate::link_state::LinkState;
use crate::notice::Notice;
use crate::outgoing::Outgoing;
use crate::overlay_frame::OverlayFrame;
use crate::page_refresh;
use crate::pick_lists;
use crate::picker::{Item, Pick, Picker, PickerEvent};
use crate::refresher::{Refresher, SETTLE};
use crate::stage_tabs;
use crate::term_pane::{FONT, PaneEvent, TermPane};
use crate::terminal_link::TerminalLink;
use crate::theme::*;
use crate::view_urls::{AuthCookie, auth_cookie, paths_text, view_url};
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
        PickSession
    ]
);

const FIRST_IDS: u64 = 1;
/// The title bar's and the status bar's heights.
const TITLE_BAR: f32 = 44.;
const STATUS_BAR: f32 = 26.;
/// Messages a chat opens with (older ones come as you ask).
const HISTORY: u32 = 60;

pub struct Shell {
    /// The palette or a picker, over the window.
    overlay: Option<Entity<Picker>>,
    endpoint: Endpoint,
    connection: Connection,
    ids: Arc<AtomicU64>,
    store: Store,
    link: LinkState,
    chat: Entity<ChatPane>,
    /// The home, shown instead of the chat and the stage.
    home: Option<Entity<HomeView>>,
    /// Stage terminals, by terminal id.
    panes: HashMap<String, Entity<TermPane>>,
    /// Browser panes of the other tabs; made when a tab is first shown.
    webs: BrowserTabs<Entity<WebPane>>,
    /// Which tabs must show a page or a diff again after a change on the server, and when (the
    /// rules and their reasons: [crate::refresher]).
    refresh: Refresher,
    /// A timer for the reloads above is running. A reconnect leaves the sleeping one alone: it
    /// fires, finds nothing pending, and re-arms only if something waits by then.
    refresh_armed: bool,
    subscriptions: Vec<Subscription>,
    chat_width: ChatWidth,
    dragging: bool,
    /// Show what the agent opens (else it is only marked unseen). The client's choice.
    follow: bool,
    /// The outside agents (ACP) the server can run an agentic session with.
    harnesses: Vec<String>,
    /// The shown session's background processes.
    processes: Vec<ProcessInfo>,
    /// The window is in front (as of its last draw): notifications only when it isn't.
    active: bool,
    /// Drawn once (start-up timing).
    drawn: bool,
    /// A session was just shown: the chat takes the keyboard on the next draw.
    focus_chat: bool,
    /// Where the pointer was: a panel takes the keyboard when the pointer moves over it.
    hover: HoverFocus,
    visibility_observed: bool,
    window_visible: bool,
    focus: FocusHandle,
}

impl Shell {
    pub fn new(
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
            overlay: None,
            endpoint,
            connection,
            ids: Arc::new(AtomicU64::new(FIRST_IDS)),
            store: Store::default(),
            link: LinkState::Connecting,
            chat,
            home: None,
            panes: HashMap::new(),
            webs: BrowserTabs::default(),
            refresh: Refresher::default(),
            refresh_armed: false,
            subscriptions,
            chat_width: ChatWidth::DEFAULT,
            dragging: false,
            follow: true,
            harnesses: Vec::new(),
            processes: Vec::new(),
            active: true,
            drawn: false,
            focus_chat: true,
            hover: HoverFocus::default(),
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
                // First, ahead of anything that could make the server broadcast: `page.changed`
                // reaches only a socket that asked for a URL, and this window shows whatever ends
                // up on the stage, so it asks for everything (as the TUI does on connect) and
                // routes what arrives to the tabs it is about.
                self.ask(
                    ClientCommand::Subscribe {
                        urls: vec!["*".into()],
                    },
                    cx,
                );
                self.store.reconnected();
                self.sync_approvals(cx);
                self.sync_home(cx);
                // All the sessions, for the switcher (updates follow as events); the first
                // reply also picks the session to show.
                self.ask(ClientCommand::SessionList, cx);
                self.fetch_harnesses(cx);
                // Stage panes start over from the server's scrollback.
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
                    self.ask(
                        ClientCommand::DeskGet {
                            session: session.clone(),
                        },
                        cx,
                    );
                    self.fetch_processes(session, cx);
                }
                // Nothing on screen is known to be current after this. Every pane is made again
                // just above, so nothing is left showing what it loaded before the break.
                self.refresh.clear();
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
            // Something a stage tab shows changed on the server: those tabs follow, on their own
            // schedule. Which tabs (`page_refresh`, matched on the ids the server normalised) and
            // when (`refresher`, coalesced and kind to the reader) are both decided without a
            // browser; this only routes and asks.
            ServerMessage::PageChanged { url, .. } => {
                self.changed(page_refresh::page_tabs(&self.store.desk, url), cx)
            }
            ServerMessage::SessionDeleted { session } => self.deleted(session.clone(), cx),
            ServerMessage::ProcessesChanged { session, processes } if self.is_shown(session) => {
                self.processes = processes.clone()
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

    /// A session is gone: out of the lists; when it was the shown one, another is shown.
    fn deleted(&mut self, session: String, cx: &mut Context<Self>) {
        self.store.sessions.retain(|s| s.id != session);
        self.store.approvals.retain(|_, a| a.session != session);
        if self.is_shown(&session) {
            self.store.attached = None;
            self.processes.clear();
            self.pick_first_session(cx);
        }
        self.sync_approvals(cx);
        self.sync_home(cx);
    }

    /// The harnesses the server offers (`/api/info`), for the new-session menu.
    fn fetch_harnesses(&mut self, cx: &mut Context<Self>) {
        let endpoint = self.endpoint.clone();
        let fetched = cx.global::<Desktop>().spawn(async move {
            endpoint
                .get::<composer_protocol::http::ServerInfo>("/api/info")
                .await
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(info)) = fetched.await {
                let _ = this.update(cx, |shell, _| shell.harnesses = info.harnesses);
            }
        })
        .detach();
    }

    /// `session`'s background processes, while it is the shown one.
    fn fetch_processes(&mut self, session: String, cx: &mut Context<Self>) {
        let connection = self.connection.clone();
        cx.spawn(async move |this, cx| {
            let answer = connection
                .request(ClientCommand::ProcessList {
                    session: session.clone(),
                })
                .await;
            let _ = this.update(cx, |shell, cx| {
                if let Ok(CommandResult::Processes { processes }) = answer
                    && shell.is_shown(&session)
                {
                    shell.processes = processes;
                    cx.notify();
                }
            });
        })
        .detach();
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
        self.focus_chat = true;
        self.processes.clear();
        self.fetch_processes(session.clone(), cx);
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

    /// The first session list: show the latest session, or start one.
    fn pick_first_session(&mut self, cx: &mut Context<Self>) {
        if self.store.attached.is_some() {
            return;
        }
        let latest = self
            .store
            .sessions
            .iter()
            .max_by(|a, b| a.updated.cmp(&b.updated))
            .map(|s| s.id.clone());
        match latest {
            Some(session) => self.show_session(session, cx),
            None => self.new_session(RunKind::Agentic, None, None, cx),
        }
    }

    /// The new-session menu: a run kind, in the shown session's site, no site or another one
    /// (the other sites join when fetched).
    fn open_new_session_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.session_site().map(str::to_string);
        let harnesses = self.harnesses.clone();
        let items = pick_lists::new_sessions(&[], current.as_deref(), &harnesses);
        let shown = items.len();
        let picker = cx.new(|cx| Picker::new("new session", items, cx));
        let endpoint = self.endpoint.clone();
        let fetched = cx.global::<Desktop>().spawn(async move {
            endpoint
                .get::<Vec<composer_protocol::http::SiteSummary>>("/api/sites")
                .await
                .unwrap_or_default()
        });
        let weak = picker.downgrade();
        cx.spawn(async move |_, cx| {
            let Ok(sites) = fetched.await else {
                return;
            };
            // The kinds already listed (this site, no site) come first; the other sites follow.
            let more =
                pick_lists::new_sessions(&sites, current.as_deref(), &harnesses).split_off(shown);
            let _ = weak.update(cx, |picker, cx| picker.extend(more, cx));
        })
        .detach();
        self.show_picker(picker, window, cx);
    }

    /// Start a session of `kind`, in `site` or none, run by `harness` or composer.
    fn new_session(
        &mut self,
        kind: RunKind,
        site: Option<String>,
        harness: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let connection = self.connection.clone();
        cx.spawn(async move |this, cx| {
            let created = connection
                .request(pick_lists::new_session(kind, site, harness))
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

    /// The shown session's state and site, for the chat.
    fn sync_info(&mut self, cx: &mut Context<Self>) {
        let info = self
            .store
            .attached
            .as_ref()
            .and_then(|id| self.store.sessions.iter().find(|s| &s.id == id))
            .map(|s| (s.state, s.site.clone()));
        if let Some((state, site)) = info {
            self.chat
                .update(cx, |chat, cx| chat.set_info(state, site, cx));
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

    /// Shows the home (instead of the chat and the stage).
    fn open_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let home = match self.home.clone() {
            Some(home) => home,
            None => {
                let connection = self.connection.clone();
                let home = cx.new(|cx| HomeView::new(connection, cx));
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
            HomeEvent::NewSession => {
                self.close_home(window, cx);
                self.open_new_session_menu(window, cx);
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
            // To the session it was written in (the chat's).
            ChatEvent::Send { send, attachments } => self.send_message(send, attachments, cx),
            ChatEvent::Answer { request, decision } => {
                self.ask(ClientCommand::ApprovalRespond { request, decision }, cx)
            }
            ChatEvent::Cancel => {
                // What it had seen: a turn that started since (a queued message) keeps going.
                let seen = self.chat.read(cx).seen(&session);
                self.ask(ClientCommand::TurnCancel { session, seen }, cx)
            }
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

    /// A message from the chat.
    fn send_message(
        &mut self,
        send: Outgoing,
        attachments: Vec<composer_protocol::Attachment>,
        cx: &mut Context<Self>,
    ) {
        let connection = self.connection.clone();
        let command = ClientCommand::MessageSend {
            session: send.session.clone(),
            text: send.text.clone(),
            pages: vec![],
            images: vec![],
            attachments,
        };
        cx.spawn(async move |this, cx| {
            let answer = connection.request(command).await;
            let _ = this.update(cx, |shell, cx| shell.sent(answer, cx));
        })
        .detach();
    }

    /// The server's answer to a message from the chat.
    fn sent(&mut self, answer: Result<CommandResult, String>, cx: &mut Context<Self>) {
        match answer {
            Ok(result) => self.on_result(result, cx),
            Err(e) => self.link = self.link.refused(e),
        }
        cx.notify();
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
        self.refresh.keep(&tabs);
        for id in on_desk {
            if self.panes.contains_key(&id) {
                continue;
            }
            let pane = self.new_pane(id.clone(), cx);
            self.panes.insert(id.clone(), pane.clone());
            self.read_terminal(id, pane, cx);
        }
        // A tab that is shown now but changed on the server while it was hidden catches up; one
        // with no pane yet needs nothing, it loads its page as it is made. A reload that is still
        // settling for the tab already on screen is not applied here: this runs on every desk
        // change, and cutting its wait short would reload a page mid-read (see `Refresher::focus`).
        let shown = stage_tabs::shown(&self.store.desk).map(str::to_string);
        if let Some(shown) = shown
            && self.refresh.focus(&shown)
        {
            self.load_again(&shown, cx);
        }
    }

    /// Tabs whose content just changed on the server. The one on screen waits a moment first, so a
    /// burst of events for one page is one reload; the others are only marked, and the reload
    /// happens when one of them is shown. Nothing on screen changes for a page no tab shows.
    fn changed(&mut self, tabs: Vec<String>, cx: &mut Context<Self>) {
        if tabs.is_empty() {
            return;
        }
        let shown = stage_tabs::shown(&self.store.desk).map(str::to_string);
        for tab in tabs {
            self.refresh.changed(&tab, shown.as_deref(), Instant::now());
        }
        self.arm_refresh(cx);
        cx.notify(); // a hidden tab's mark is drawn on its tab
    }

    /// One timer at a time, for as long as a tab is waiting.
    fn arm_refresh(&mut self, cx: &mut Context<Self>) {
        if self.refresh_armed || !self.refresh.waiting() {
            return;
        }
        self.refresh_armed = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            let _ = this.update(cx, |shell, cx| {
                shell.refresh_armed = false;
                shell.refresh_due(cx);
            });
        })
        .detach();
    }

    /// Every tab whose wait has passed. `Refresher` decides what happens to each one — load it,
    /// wait one more round for a reader holding a mouse button down in the page, or leave it marked
    /// because nobody is looking at it any more — and only the first kind comes back here. A tab
    /// with no pane is simply current: its page loads when the pane is made.
    fn refresh_due(&mut self, cx: &mut Context<Self>) {
        let shown = stage_tabs::shown(&self.store.desk).map(str::to_string);
        let webs = &self.webs;
        let reload = self
            .refresh
            .take_due(Instant::now(), shown.as_deref(), |tab| {
                webs.get(tab)
                    .is_some_and(|pane| pane.read(cx).interacting())
            });
        for tab in reload {
            self.load_again(&tab, cx);
        }
        self.arm_refresh(cx);
        cx.notify(); // a tab's mark appears, or is gone now that its page is current
    }

    /// A tab's browser pane, if it has one, showing its page as it is on the server now.
    fn load_again(&mut self, tab: &str, cx: &mut Context<Self>) {
        if let Some(pane) = self.webs.get(tab).cloned() {
            pane.update(cx, |pane, _| pane.refresh());
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
                command: None,
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
                command: None,
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

    /// The keyboard stays with something on screen: an open picker, the home, the chat or the
    /// stage's shown tab (the rules: [crate::focus_rule]). When the shown tab changes while a
    /// pane now hidden has the keyboard, the shown pane takes it; when what had it is gone (a
    /// closed picker's way back, a closed tab), the chat does. A session just shown gives it to
    /// the chat.
    fn keep_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let overlay = self.overlay.as_ref().map(|p| p.read(cx).focus_handle(cx));
        let home = self.home.as_ref().map(|h| h.read(cx).focus_handle(cx));
        let chat = self.chat.read(cx).focus_handle(cx);
        let shown_term = self.shown_pane().cloned();
        let shown_web = self.shown_web().cloned();
        let stage = shown_term
            .as_ref()
            .map(|pane| pane.read(cx).focus_handle(cx))
            .or_else(|| shown_web.as_ref().map(|web| web.read(cx).focus_handle(cx)));
        let focused =
            |handle: &Option<FocusHandle>| handle.as_ref().is_some_and(|h| h.is_focused(window));
        let holder = if focused(&overlay) {
            Holder::Overlay
        } else if focused(&home) {
            Holder::Home
        } else if chat.is_focused(window) {
            Holder::Chat
        } else if focused(&stage) {
            Holder::ShownStage
        } else {
            let shown_tab = stage_tabs::shown(&self.store.desk);
            let hidden = self
                .panes
                .values()
                .filter(|pane| Some(pane.entity_id()) != shown_term.as_ref().map(|p| p.entity_id()))
                .map(|pane| pane.read(cx).focus_handle(cx))
                .chain(
                    self.webs
                        .hidden(shown_tab)
                        .map(|web| web.read(cx).focus_handle(cx)),
                )
                .any(|hidden| hidden.is_focused(window));
            if hidden {
                Holder::HiddenStage
            } else {
                Holder::Other
            }
        };
        let scene = Scene {
            overlay: overlay.is_some(),
            home: home.is_some(),
            stage: stage.is_some(),
        };
        // (Kept while a picker or the home is open: it applies once they close.)
        let wanted = if self.focus_chat && !scene.overlay && !scene.home {
            self.focus_chat = false;
            (holder != Holder::Chat).then_some(Target::Chat)
        } else {
            focus_rule::target(scene, holder)
        };
        let handle = match wanted {
            None => return,
            Some(Target::Overlay) => overlay,
            Some(Target::Home) => home,
            Some(Target::Chat) => Some(chat),
            Some(Target::ShownStage) => stage,
        };
        let Some(handle) = handle else {
            return;
        };
        window.focus(&handle, cx);
        if wanted == Some(Target::ShownStage)
            && shown_term.is_none()
            && let Some(web) = shown_web
        {
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

    /// The shown tab's browser pane, if it is a browser tab and has none yet.
    fn load_shown(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.shown_entry().cloned() else {
            return;
        };
        if matches!(entry.tab, DeskTab::Terminal { .. }) || !self.webs.begin(&entry.id) {
            return;
        }
        let Some(url) = view_url(&self.endpoint, &entry.tab) else {
            return;
        };
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
            move |shell, from, event: &PickerEvent, window, cx| {
                // A click on a row lands on the scrim under it too, and its late `Dismissed`
                // must not close the picker the row just opened.
                if shell.overlay.as_ref() != Some(from) {
                    return;
                }
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
            group: "home",
            pick: Pick::Home,
        });
        let picker = cx.new(|cx| Picker::new("session", items, cx));
        self.show_picker(picker, window, cx);
    }

    /// The shown session's background processes: open one's terminal, or stop it.
    fn open_processes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = pick_lists::processes(&self.processes, crate::home_lists::now_secs());
        let picker = cx.new(|cx| Picker::new("process", items, cx));
        self.show_picker(picker, window, cx);
    }

    fn shown_info(&self) -> Option<&composer_protocol::SessionInfo> {
        let id = self.store.attached.as_ref()?;
        self.store.sessions.iter().find(|s| &s.id == id)
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

    /// Commands, then sessions; the sites join when fetched.
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
            endpoint
                .get::<Vec<composer_protocol::http::SiteSummary>>("/api/sites")
                .await
                .unwrap_or_default()
        });
        let weak = picker.downgrade();
        cx.spawn(async move |this, cx| {
            let Ok(sites) = fetched.await else {
                return;
            };
            // The current site may have changed while the catalog was fetched.
            let mut more = this
                .update(cx, |shell, _| {
                    pick_lists::sites(&sites, shell.session_site())
                })
                .unwrap_or_default();
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
            Pick::Session(session) => self.show_session(session, cx),
            Pick::NewSession => self.open_new_session_menu(window, cx),
            Pick::CreateSession {
                kind,
                site,
                harness,
            } => self.new_session(kind, site, harness, cx),
            Pick::DeleteMenu => {
                if let Some(info) = self.shown_info() {
                    let items = pick_lists::delete_confirm(info);
                    let picker = cx.new(|cx| Picker::new("delete session?", items, cx));
                    self.show_picker(picker, window, cx);
                }
            }
            Pick::DeleteSession(session) => self.ask(ClientCommand::SessionDelete { session }, cx),
            Pick::ProcessMenu => self.open_processes(window, cx),
            // Its terminal, as a stage tab.
            Pick::OpenProcess(terminal) => {
                if let Some(session) = self.store.attached.clone() {
                    self.ask(
                        ClientCommand::DeskOpen {
                            session,
                            tab: DeskTab::Terminal { id: terminal },
                            focus: true,
                        },
                        cx,
                    );
                }
            }
            Pick::StopProcess(terminal) => self.ask(ClientCommand::ProcessStop { terminal }, cx),
            Pick::Page(url) => self.open_page(url, cx),
            Pick::Site(site) => self.switch_site(Some(site), cx),
            Pick::LeaveSite => self.switch_site(None, cx),
            Pick::NewTerminal => self.new_terminal(cx),
            Pick::AttachFile => self.attach_file(cx),
            Pick::ToggleFollow => self.set_follow(!self.follow, cx),
            Pick::Sessions => self.open_sessions(window, cx),
            Pick::ModelMenu => self.open_models_menu(window, cx),
            Pick::Compact => {
                if let Some(session) = self.store.attached.clone() {
                    self.ask(ClientCommand::SessionCompact { session }, cx);
                }
            }
            Pick::SetModel(model) => {
                if let Some(session) = self.store.attached.clone() {
                    self.ask(ClientCommand::ModelSet { session, model }, cx);
                }
            }
            Pick::Home => self.open_home(window, cx),
        }
    }

    /// Follow the agent (show what it opens, keep its newest messages in view), or stop.
    fn set_follow(&mut self, follow: bool, cx: &mut Context<Self>) {
        self.follow = follow;
        self.chat
            .update(cx, |chat, cx| chat.set_following(follow, cx));
        cx.notify();
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

    /// `site.set` for the shown session: enters `site`, or leaves the current one. What the server answered,
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
            && let Some(notice) = Notice::of(message, self.store.attached.as_deref())
        {
            notice.show();
        }
    }

    /// The shown session for the title bar: its run kind and id, and its site.
    fn title_parts(&self) -> (String, String, String) {
        let session = self
            .store
            .attached
            .as_ref()
            .and_then(|id| self.store.sessions.iter().find(|s| &s.id == id));
        let kind = session
            .map(pick_lists::kind_label)
            .unwrap_or_else(|| "no session".into());
        let id = session.map(|s| format!("#{}", s.id)).unwrap_or_default();
        let site = session
            .and_then(|s| s.site.clone())
            .unwrap_or_else(|| "no site".into());
        (kind, id, site)
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
        }
        let total = f32::from(window.viewport_size().width);
        self.chat_width = self.chat_width.within(total);
        let active = window.is_window_active();
        if active && !self.active {
            // Back in front: the pointer arriving over a panel doesn't take the keyboard.
            self.hover.reset();
        }
        self.active = active;
        self.load_shown(cx);
        self.sync_web_visibility(cx);
        // Keys go to the chat from the start (the rules make it so).
        self.keep_focus(window, cx);
        let shown = stage_tabs::shown(&self.store.desk).map(str::to_string);
        let tabs = self.store.desk.tabs.iter().map(|entry| {
            let id = entry.id.clone();
            let close = entry.id.clone();
            let active = shown.as_deref() == Some(entry.id.as_str());
            div()
                .id(gpui::SharedString::from(format!("tab-{}", entry.id)))
                .relative()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(9.))
                .px(px(18.))
                // `base.css`: `border-top: 2px transparent`, `border-right: 1px border`, the shown
                // tab's top edge in the accent. gpui has one border colour per element, so the
                // divider is the border (every tab, the same colour) and the accent is a bar of
                // its own, inside the padding box: it never reaches the divider.
                .pt(px(2.))
                .border_r_1()
                .border_color(rgb(BORDER))
                .text_size(px(TEXT_SIZE))
                .when(active, |d| {
                    d.bg(rgb(BG_DEEP)).text_color(rgb(FG_BRIGHT)).child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(2.))
                            .bg(rgb(ACCENT)),
                    )
                })
                .when(!active, |d| {
                    d.text_color(rgb(FG_DIM))
                        .hover(|d| d.text_color(rgb(FG_BRIGHT)))
                })
                .on_click(cx.listener(move |shell, _, _, cx| shell.show_tab(id.clone(), cx)))
                .when(entry.unseen, |d| {
                    d.child(div().size(px(6.)).rounded(px(R_DOT)).bg(rgb(ACCENT_SOFT)))
                })
                // Its page or diff changed on the server while this tab was hidden; it loads the
                // new text the moment it is shown (an outlined dot, next to the filled one).
                .when(self.refresh.is_stale(&entry.id), |d| {
                    d.child(
                        div()
                            .size(px(6.))
                            .rounded(px(R_DOT))
                            .border_1()
                            .border_color(rgb(ACCENT_SOFT)),
                    )
                })
                .child(stage_tabs::label(&entry.tab, self.session_site()))
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
                shell.set_follow(!shell.follow, cx);
            }));
        let stage_body = match (self.shown_pane(), self.shown_web(), shown.is_some()) {
            (Some(pane), _, _) => pane.clone().into_any_element(),
            (None, Some(web), _) => web.clone().into_any_element(),
            (None, None, true) => placeholder("Opening…").into_any_element(),
            (None, None, false) => placeholder(
                "Nothing on the stage. The agent can put pages here; + opens a terminal.",
            )
            .into_any_element(),
        };
        let chat = self.chat.clone().into_any_element();
        let (kind, session_id, site) = self.title_parts();
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
        // The palette and pickers float dead centre of the window, between the two bars.
        let viewport = window.viewport_size();
        let window_width = f32::from(viewport.width);
        let frame = OverlayFrame::within(
            window_width,
            f32::from(viewport.height) - TITLE_BAR - STATUS_BAR,
        );

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
                cx.listener(|shell, _: &PickSession, window, cx| shell.open_sessions(window, cx)),
            )
            .on_action(cx.listener(|shell, _: &Home, window, cx| shell.open_home(window, cx)))
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
                // After the panels' own listeners (this is their parent): they compare with it.
                shell.hover.seen_at(ev.position);
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
            // The title bar: the home, the shown session, the link and follow; Sessions and
            // Commands at the right.
            .child(
                div()
                    .h(px(TITLE_BAR))
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
                    .child(div().size(px(8.)).rounded(px(R_DOT)).bg(rgb(ACCENT)))
                    .child(
                        // Opens the home.
                        div()
                            .id("home-name")
                            .text_color(rgb(FG_BRIGHT))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|d| d.text_color(rgb(ACCENT_SOFT)))
                            .on_click(
                                cx.listener(|shell, _, window, cx| shell.open_home(window, cx)),
                            )
                            .child("composer"),
                    )
                    .child(div().text_color(rgb(FG_FAINT)).child("/"))
                    .when(in_home, |d| {
                        d.child(div().text_color(rgb(FG)).child("home"))
                    })
                    .when(!in_home, |d| {
                        d.child(div().text_color(rgb(FG)).child(kind.clone()))
                            .child(div().text_color(rgb(FG_FAINT)).child(session_id.clone()))
                            .child(div().text_color(rgb(FG_FAINT)).child(site))
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
                                .on_mouse_move(cx.listener(
                                    |shell, ev: &MouseMoveEvent, window, cx| {
                                        if shell.overlay.is_some()
                                            || !shell.hover.moves_to(ev.position)
                                        {
                                            return;
                                        }
                                        let handle = shell.chat.read(cx).focus_handle(cx);
                                        if !handle.is_focused(window) {
                                            window.focus(&handle, cx);
                                        }
                                    },
                                ))
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
                                        .on_mouse_move(cx.listener(
                                            |shell, ev: &MouseMoveEvent, window, cx| {
                                                if shell.overlay.is_some()
                                                    || !shell.hover.moves_to(ev.position)
                                                {
                                                    return;
                                                }
                                                if let Some(pane) = shell.shown_pane() {
                                                    let handle = pane.read(cx).focus_handle(cx);
                                                    if !handle.is_focused(window) {
                                                        window.focus(&handle, cx);
                                                    }
                                                } else if let Some(web) = shell.shown_web().cloned()
                                                {
                                                    let handle = web.read(cx).focus_handle(cx);
                                                    if !handle.is_focused(window) {
                                                        window.focus(&handle, cx);
                                                        // CEF has its own focus: without it the page sees no keys.
                                                        web.update(cx, |web, _| {
                                                            web.browser_focus(true)
                                                        });
                                                    }
                                                }
                                            },
                                        ))
                                        .child(stage_body),
                                ),
                        ),
                )
            })
            // The status bar: what waits for you; the keys at the right.
            .child(
                div()
                    .h(px(STATUS_BAR))
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
                    .when(waiting > 0, |d| {
                        d.child(
                            div()
                                .text_color(rgb(WARN))
                                .child(format!("! {waiting} waiting")),
                        )
                    })
                    .when(!self.processes.is_empty(), |d| {
                        let count = self.processes.len();
                        d.child(
                            div()
                                .id("status-processes")
                                .text_color(rgb(FG_DIM))
                                .hover(|d| d.text_color(rgb(FG_BRIGHT)))
                                .child(format!(
                                    "⚙ {count} process{} running",
                                    if count == 1 { "" } else { "es" }
                                ))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.open_processes(window, cx)
                                })),
                        )
                    })
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
            // The palette or a picker, over both panes (painted last): the window dims under it,
            // the bars stay as they are.
            .children(self.overlay.clone().map(|picker| {
                div()
                    .absolute()
                    .top(px(TITLE_BAR))
                    .bottom(px(STATUS_BAR))
                    .left_0()
                    .right_0()
                    .flex()
                    .flex_row()
                    .items_center()
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
                        // The panel takes its own clicks (the query line, a head, a gap between
                        // rows): only a click beside it, on the scrim, closes the picker.
                        div()
                            .id("picker-panel")
                            .occlude()
                            .ml(px(frame.left(window_width)))
                            .w(px(frame.width()))
                            .max_h(px(frame.max_height()))
                            .flex()
                            .flex_col()
                            .child(picker),
                    )
            }))
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
