//! A workspace's window: the title bar, the chat column (the window's own TUI, in a terminal pane)
//! and the stage (the attached session's desk as tabs).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use base64::Engine;
use composer_client::{Connection, Endpoint, Event, Store};
use composer_protocol::{ClientCommand, CommandResult, DeskEntry, DeskTab, ServerMessage};
use gpui::{
    Context, Entity, ExternalPaths, FocusHandle, Focusable, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Subscription, Window, actions, div, prelude::*, px, rgb,
};
use tokio::sync::mpsc;

use crate::browser_tabs::BrowserTabs;
use crate::chat_width::ChatWidth;
use crate::desktop::{self, Desktop};
use crate::link_state::LinkState;
use crate::pick_lists;
use crate::picker::{Item, Pick, Picker, PickerEvent};
use crate::stage_tabs;
use crate::term_pane::{FONT, PaneEvent, TermPane};
use crate::terminal_link::TerminalLink;
use crate::view_urls::{origin, paths_text, view_url};
use crate::web_pane::{WebEvent, WebPane};

actions!(
    desktop,
    [
        FocusChat,
        FocusStage,
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

/// The size a window's TUI starts at, before the pane has laid out.
const FIRST_GRID: (u16, u16) = (100, 40);
const FIRST_IDS: u64 = 1;

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
    link: LinkState,
    chat: Option<Entity<TermPane>>,
    /// Stage terminals, by terminal id.
    panes: HashMap<String, Entity<TermPane>>,
    /// Browser panes of the other tabs; made when a tab is first shown.
    webs: BrowserTabs<Entity<WebPane>>,
    subscriptions: Vec<Subscription>,
    chat_width: ChatWidth,
    dragging: bool,
    /// Show what the agent opens (else it is only marked unseen). The client's choice.
    follow: bool,
    /// The window is in front (as of its last draw): notifications only when it isn't.
    active: bool,
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
        Self {
            workspace,
            colour,
            overlay: None,
            endpoint,
            connection,
            ids: Arc::new(AtomicU64::new(FIRST_IDS)),
            store: Store::default(),
            link: LinkState::Connecting,
            chat: None,
            panes: HashMap::new(),
            webs: BrowserTabs::default(),
            subscriptions: Vec::new(),
            chat_width: ChatWidth::DEFAULT,
            dragging: false,
            follow: true,
            active: true,
            focus: cx.focus_handle(),
        }
    }

    fn on_event(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Connected => {
                self.link = LinkState::Up;
                self.hello(cx);
                // All the sessions, for the switcher (updates follow as events).
                self.ask(ClientCommand::SessionList, cx);
                // Stage panes start over from the server's scrollback; apps get fresh tokens.
                self.panes.clear();
                self.webs.clear();
                if let Some(session) = self.store.attached.clone() {
                    self.ask(
                        ClientCommand::SessionAttach {
                            session: session.clone(),
                            since: None,
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
            && let Some(pane) = self.pane(terminal)
        {
            pane.update(cx, |pane, cx| pane.set_ended(cx));
        }
        for command in self.store.apply(&message) {
            self.ask(command, cx);
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

    /// Asks the server for this window's TUI.
    fn hello(&mut self, cx: &mut Context<Self>) {
        let (cols, rows) = self
            .chat
            .as_ref()
            .map(|chat| chat.read(cx).grid())
            .filter(|(cols, _)| *cols > 0)
            .map(|(c, r)| (c as u16, r as u16))
            .unwrap_or(FIRST_GRID);
        self.ask(ClientCommand::WindowHello { cols, rows }, cx);
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
            CommandResult::Window { terminal, .. } => {
                let chat = self.new_pane(terminal, cx);
                self.chat = Some(chat);
            }
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

    fn pane(&self, terminal: &str) -> Option<&Entity<TermPane>> {
        match &self.chat {
            Some(chat)
                if self
                    .store
                    .window
                    .as_ref()
                    .is_some_and(|w| w.terminal == terminal) =>
            {
                Some(chat)
            }
            _ => self.panes.get(terminal),
        }
    }

    fn feed(&mut self, terminal: &str, data: &str, cx: &mut Context<Self>) {
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
            return;
        };
        if let Some(pane) = self.pane(terminal).cloned() {
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
            self.link = self
                .link
                .refused("no session yet: the chat hasn't started one".into());
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
            self.show_web(entry.id, url, cx);
            return;
        }
        let DeskTab::App { site, project } = entry.tab else {
            return;
        };
        let session = self.store.attached.clone();
        let command = ClientCommand::AppOpen {
            site,
            project,
            session: session.clone(),
        };
        let connection = self.connection.clone();
        cx.spawn(async move |this, cx| {
            let answer = connection.request(command).await;
            let _ = this.update(cx, |shell, cx| match answer {
                Ok(CommandResult::AppOpened { opened }) => {
                    let mut url = format!("{}{}", origin(shell.endpoint.base()), opened.url);
                    if let Some(session) = session {
                        url.push_str(&format!("&session={session}"));
                    }
                    shell.show_web(entry.id, url, cx);
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

    fn show_web(&mut self, tab: String, url: String, cx: &mut Context<Self>) {
        let base = self.endpoint.base().to_string();
        let pane = cx.new(|cx| WebPane::new(&url, &base, cx));
        self.subscriptions.push(cx.subscribe(
            &pane,
            |shell, _, event: &WebEvent, cx| match event {
                WebEvent::OpenPage(url) => shell.open_page(url.clone(), cx),
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
        let back = window.focused(cx).unwrap_or_else(|| self.focus.clone());
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
        let items = pick_lists::sessions(&self.store.sessions, self.store.attached.as_deref());
        let picker = cx.new(|cx| Picker::new("session", items, cx));
        self.show_picker(picker, window, cx);
    }

    /// Commands, then sessions; the workspace's sites and Home apps join when fetched.
    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut items = pick_lists::commands(self.follow);
        items.extend(pick_lists::sessions(
            &self.store.sessions,
            self.store.attached.as_deref(),
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
        cx.spawn(async move |_, cx| {
            let Ok((sites, apps, launchers)) = fetched.await else {
                return;
            };
            let mut more: Vec<Item> = apps
                .map(|list| list.apps)
                .unwrap_or_default()
                .into_iter()
                .map(|app| Item {
                    label: format!("app: {}", app.label),
                    detail: app.site.clone(),
                    pick: Pick::App(app.site),
                })
                .collect();
            more.extend(launchers.iter().filter_map(|l| {
                let name = l["name"].as_str()?.to_string();
                Some(Item {
                    label: format!("launch: {name}"),
                    detail: l["command"].as_str().unwrap_or_default().to_string(),
                    pick: Pick::Launch(name),
                })
            }));
            more.extend(sites.into_iter().map(|site| Item {
                label: format!("page: {}", site.title),
                detail: format!("site://{}", site.name),
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
            Pick::Session(session) => match self.store.window.clone() {
                // The window's TUI attaches to it; the stage follows when it says so.
                Some(binding) => self.ask(
                    ClientCommand::WindowSession {
                        window: binding.window,
                        session,
                    },
                    cx,
                ),
                None => self.link = self.link.refused("the chat hasn't started yet".into()),
            },
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
            Pick::NewTerminal => self.new_terminal(cx),
            Pick::Launch(name) => self.launch(name, cx),
            Pick::AttachFile => self.attach_file(cx),
            Pick::ToggleFollow => self.follow = !self.follow,
            Pick::Workspaces => self.open_workspaces(window, cx),
            Pick::Sessions => self.open_sessions(window, cx),
        }
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

    /// Files from the desktop's file picker go into the chat as their paths (the TUI makes
    /// attachments of pasted paths).
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
            let text = paths_text(paths.iter().map(|p| p.as_path()));
            let _ = this.update(cx, |shell, cx| shell.paste_into_chat(&text, cx));
        })
        .detach();
    }

    fn paste_into_chat(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(chat) = self.chat.clone() {
            chat.update(cx, |chat, cx| chat.paste(text, cx));
        }
    }

    /// A finished turn or an approval needed, while the window is behind: a notification.
    fn notify_if_away(&self, message: &ServerMessage) {
        if self.active {
            return;
        }
        let (summary, body) = match message {
            ServerMessage::TurnDone { session, .. }
                if self.store.attached.as_ref() == Some(session) =>
            {
                (
                    "The agent is done",
                    format!("{} · #{session}", self.workspace),
                )
            }
            ServerMessage::ApprovalRequest {
                session,
                title,
                site,
                recipe,
                ..
            } => {
                let what = if title.is_empty() {
                    format!("{site} › {recipe}")
                } else {
                    title.clone()
                };
                (
                    "An approval is waiting",
                    format!("{what} · {} · #{session}", self.workspace),
                )
            }
            _ => return,
        };
        // The notification service may block: off the window's thread.
        std::thread::spawn(move || {
            let _ = notify_rust::Notification::new()
                .appname("composer")
                .summary(summary)
                .body(&body)
                .show();
        });
    }

    fn title(&self) -> String {
        let session = self
            .store
            .attached
            .as_ref()
            .and_then(|id| self.store.sessions.iter().find(|s| &s.id == id));
        let project = session
            .and_then(|s| s.project.clone())
            .unwrap_or_else(|| "no project".into());
        format!("▸ {project}{}", self.link.suffix())
    }
}

impl Focusable for Shell {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

const BG: u32 = 0x0c0c0f;
const LINE: u32 = 0x25252e;
const TEXT: u32 = 0xd7d7dd;
const MUTED: u32 = 0x85858f;
const ACTIVE_BG: u32 = 0x1c1c24;
const ACCENT: u32 = 0xe0a040;

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let total = f32::from(window.viewport_size().width);
        self.chat_width = self.chat_width.within(total);
        self.active = window.is_window_active();
        self.load_shown(cx);
        let shown = stage_tabs::shown(&self.store.desk).map(str::to_string);
        let tabs = self.store.desk.tabs.iter().map(|entry| {
            let id = entry.id.clone();
            let close = entry.id.clone();
            let active = shown.as_deref() == Some(entry.id.as_str());
            div()
                .id(gpui::SharedString::from(format!("tab-{}", entry.id)))
                .flex()
                .flex_row()
                .flex_none()
                .gap_2()
                .px_3()
                .py_1()
                .text_size(px(12.))
                .when(active, |d| d.bg(rgb(ACTIVE_BG)).text_color(rgb(TEXT)))
                .when(!active, |d| d.text_color(rgb(MUTED)))
                .on_click(cx.listener(move |shell, _, _, cx| shell.show_tab(id.clone(), cx)))
                .when(entry.unseen, |d| {
                    d.child(div().text_color(rgb(ACCENT)).child("●"))
                })
                .child(stage_tabs::label(&entry.tab))
                .child(
                    div()
                        .id(gpui::SharedString::from(format!("close-{}", entry.id)))
                        .text_color(rgb(MUTED))
                        .child("×")
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            cx.stop_propagation();
                            shell.close_tab(close.clone(), cx)
                        })),
                )
        });
        let follow_toggle = div()
            .id("follow")
            .text_color(rgb(MUTED))
            .child(if self.follow {
                "● following agent"
            } else {
                "○ not following"
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
        let chat = match &self.chat {
            Some(chat) => chat.clone().into_any_element(),
            None => placeholder("Starting the chat…").into_any_element(),
        };

        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family(FONT)
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
            .on_action(cx.listener(|shell, _: &FocusChat, window, cx| {
                if let Some(chat) = &shell.chat {
                    window.focus(&chat.read(cx).focus_handle(cx), cx);
                }
            }))
            .on_action(cx.listener(|shell, _: &FocusStage, window, cx| {
                if let Some(pane) = shell.shown_pane() {
                    window.focus(&pane.read(cx).focus_handle(cx), cx);
                } else if let Some(web) = shell.shown_web() {
                    window.focus(&web.read(cx).focus_handle(cx), cx);
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
            // The workspace's colour across the top: which workspace this is, at a glance.
            .child(div().h(px(3.)).flex_none().bg(rgb(self.colour)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .px_3()
                    .py_1()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .text_size(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(rgb(self.colour))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(format!("◉ {}", self.workspace)),
                            )
                            .child(self.title()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_4()
                            .child(
                                div()
                                    .text_color(rgb(MUTED))
                                    .child(workers_label(cx.global::<Desktop>().workers())),
                            )
                            .child(follow_toggle),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .flex()
                    .flex_row()
                    .child(
                        div()
                            .flex_none()
                            .w(px(self.chat_width.get()))
                            .h_full()
                            .min_h(px(0.))
                            .overflow_hidden()
                            // Files dropped on the chat: their paths, which the TUI attaches.
                            .on_drop(cx.listener(|shell, paths: &ExternalPaths, _, cx| {
                                let text = paths_text(paths.paths().iter().map(|p| p.as_path()));
                                shell.paste_into_chat(&text, cx);
                            }))
                            .child(chat),
                    )
                    .child(
                        div()
                            .id("chat-edge")
                            .flex_none()
                            .w(px(4.))
                            .h_full()
                            .bg(rgb(LINE))
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
                            .child(
                                div()
                                    .id("tab-strip")
                                    .flex()
                                    .flex_row()
                                    .flex_none()
                                    .overflow_x_scroll()
                                    .border_b_1()
                                    .border_color(rgb(LINE))
                                    .children(tabs)
                                    .child(
                                        div()
                                            .id("new-terminal")
                                            .px_3()
                                            .py_1()
                                            .text_size(px(12.))
                                            .text_color(rgb(MUTED))
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
                                    .on_drop(cx.listener(|shell, paths: &ExternalPaths, _, cx| {
                                        if let Some(pane) = shell.shown_pane().cloned() {
                                            let text = paths_text(
                                                paths.paths().iter().map(|p| p.as_path()),
                                            );
                                            pane.update(cx, |pane, cx| pane.paste(&text, cx));
                                        }
                                    }))
                                    .child(stage_body),
                            ),
                    ),
            )
            // The palette or a picker, over everything.
            .children(self.overlay.clone().map(|picker| {
                div()
                    .absolute()
                    .top(px(70.))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(picker)
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
        .text_size(px(13.))
        .text_color(rgb(MUTED))
        .child(text)
}
