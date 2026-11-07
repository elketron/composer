//! A workspace's window: the title bar, the chat column (the window's own TUI, in a terminal pane)
//! and the stage (the attached session's desk as tabs).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use base64::Engine;
use composer_client::{Connection, Endpoint, Event, Store};
use composer_protocol::{ClientCommand, CommandResult, DeskEntry, DeskTab, ServerMessage};
use gpui::{
    Context, Entity, FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Subscription, Window, actions, div, prelude::*, px, rgb,
};
use tokio::sync::mpsc;

use crate::chat_width::ChatWidth;
use crate::link_state::LinkState;
use crate::stage_tabs;
use crate::term_pane::{FONT, PaneEvent, TermPane};
use crate::terminal_link::TerminalLink;
use crate::view_urls::{origin, view_url};
use crate::web_pane::{WebEvent, WebPane};

actions!(desktop, [FocusChat, FocusStage, NextTab, PrevTab]);

/// The size a window's TUI starts at, before the pane has laid out.
const FIRST_GRID: (u16, u16) = (100, 40);
const FIRST_IDS: u64 = 1;

pub struct Shell {
    workspace: String,
    endpoint: Endpoint,
    connection: Connection,
    ids: Arc<AtomicU64>,
    store: Store,
    link: LinkState,
    chat: Option<Entity<TermPane>>,
    /// Stage terminals, by terminal id.
    panes: HashMap<String, Entity<TermPane>>,
    /// Browser panes of the other tabs, by tab id; made when a tab is first shown.
    webs: HashMap<String, Entity<WebPane>>,
    /// Tabs whose browser pane is on its way (`app.open` asked).
    loading: HashSet<String>,
    subscriptions: Vec<Subscription>,
    chat_width: ChatWidth,
    dragging: bool,
    focus: FocusHandle,
}

impl Shell {
    pub fn new(
        workspace: String,
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
            endpoint,
            connection,
            ids: Arc::new(AtomicU64::new(FIRST_IDS)),
            store: Store::default(),
            link: LinkState::Connecting,
            chat: None,
            panes: HashMap::new(),
            webs: HashMap::new(),
            loading: HashSet::new(),
            subscriptions: Vec::new(),
            chat_width: ChatWidth::DEFAULT,
            dragging: false,
            focus: cx.focus_handle(),
        }
    }

    fn on_event(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Connected => {
                self.link = LinkState::Up;
                self.hello(cx);
                // Stage panes start over from the server's scrollback; apps get fresh tokens.
                self.panes.clear();
                self.webs.clear();
                self.loading.clear();
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
        if let ServerMessage::TerminalExited { terminal, .. } = &message
            && let Some(pane) = self.pane(terminal)
        {
            pane.update(cx, |pane, cx| pane.set_ended(cx));
        }
        for command in self.store.apply(&message) {
            self.ask(command, cx);
        }
        if matches!(message, ServerMessage::DeskChanged { .. }) {
            self.sync_panes(cx);
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
                self.feed(&terminal, &scrollback, cx);
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
        self.webs.retain(|id, _| tabs.contains(id.as_str()));
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
        if matches!(entry.tab, DeskTab::Terminal { .. })
            || self.webs.contains_key(&entry.id)
            || !self.loading.insert(entry.id.clone())
        {
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
                    shell.loading.remove(&entry.id);
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
            },
        ));
        self.loading.remove(&tab);
        self.webs.insert(tab, pane);
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

    fn title(&self) -> String {
        let session = self
            .store
            .attached
            .as_ref()
            .and_then(|id| self.store.sessions.iter().find(|s| &s.id == id));
        let project = session
            .and_then(|s| s.project.clone())
            .unwrap_or_else(|| "no project".into());
        format!("◉ {} ▸ {project}{}", self.workspace, self.link.suffix())
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
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family(FONT)
            .key_context("shell")
            .track_focus(&self.focus)
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
            .child(
                div()
                    .px_3()
                    .py_1()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .text_size(px(12.))
                    .child(self.title()),
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
                                    .child(stage_body),
                            ),
                    ),
            )
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
