//! The home (mockups/next 11): outside any session, in place of the chat and the stage. Sessions
//! (NEEDS YOU, RECENT) on the left; the selected one on the right: its waiting approval, last
//! messages and stage tabs. It fetches what it previews itself; the sessions and approvals come
//! from the shell's store.

use std::collections::BTreeMap;

use composer_client::{Approval, Chat, ChatItem, Connection};
use composer_protocol::http::AppInfo;
use composer_protocol::{
    ApprovalDecision, ClientCommand, CommandResult, Desk, ServerMessage, SessionInfo,
};
use gpui::{
    AnyElement, Context, EventEmitter, FocusHandle, Focusable, FontWeight, KeyDownEvent,
    ScrollHandle, SharedString, Window, div, prelude::*, px, rgb,
};

use crate::chat_pane::PROSE;
use crate::home_lists::{SessionRow, asked_line, now_secs, session_rows, state_word};
use crate::pick_lists::kind_label;
use crate::stage_tabs;
use crate::theme::*;

/// What the home asks of the window.
#[derive(Debug, Clone, PartialEq)]
pub enum HomeEvent {
    /// Show this session (the home closes).
    Open(String),
    /// Back to the session shown before.
    Back,
    /// A new session.
    NewSession,
    Answer {
        request: String,
        decision: ApprovalDecision,
    },
}

/// The selected session's preview: its last messages and its desk.
struct Preview {
    session: String,
    info: SessionInfo,
    chat: Chat,
    desk: Desk,
}

pub struct HomeView {
    selected: usize,
    connection: Connection,
    sessions: Vec<SessionInfo>,
    approvals: BTreeMap<String, Approval>,
    preview: Option<Preview>,
    /// The revision being fetched, and a generation to reject late replies.
    preview_request: Option<SessionInfo>,
    preview_generation: u64,
    scroll: ScrollHandle,
    focus: FocusHandle,
    /// The server's apps, for naming app tabs.
    apps: Vec<AppInfo>,
}

impl EventEmitter<HomeEvent> for HomeView {}

impl Focusable for HomeView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Last messages a session's preview shows.
const PREVIEW_ITEMS: usize = 4;

impl HomeView {
    pub fn new(connection: Connection, apps: Vec<AppInfo>, cx: &mut Context<Self>) -> Self {
        Self {
            selected: 0,
            connection,
            sessions: Vec::new(),
            approvals: BTreeMap::new(),
            preview: None,
            preview_request: None,
            preview_generation: 0,
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            apps,
        }
    }

    /// The server's apps (`/api/info`), when they arrive after the home opened.
    pub fn set_apps(&mut self, apps: Vec<AppInfo>, cx: &mut Context<Self>) {
        self.apps = apps;
        cx.notify();
    }

    /// The store's sessions and approvals, as they change.
    pub fn set_sessions(
        &mut self,
        sessions: &[SessionInfo],
        approvals: &BTreeMap<String, Approval>,
        cx: &mut Context<Self>,
    ) {
        if self.sessions != sessions || &self.approvals != approvals {
            let before = self.selected_session();
            self.sessions = sessions.to_vec();
            self.approvals = approvals.clone();
            self.selected =
                preserve_session_selection(self.selected, before.as_deref(), &self.session_order());
            self.clamp();
            // fetch_preview compares revisions, including a request already in flight.
            self.selection_changed(cx);
            cx.notify();
        }
    }

    /// A live desk change can arrive without a new session revision.
    pub fn apply(&mut self, message: &ServerMessage, cx: &mut Context<Self>) {
        if let ServerMessage::DeskChanged { session, desk, .. } = message {
            if let Some(preview) = self.preview.as_mut().filter(|p| &p.session == session) {
                preview.desk = desk.clone();
                cx.notify();
            }
            if self
                .preview_request
                .as_ref()
                .is_some_and(|info| &info.id == session)
            {
                // The pending desk snapshot may predate this event.
                self.preview_request = None;
                self.preview_generation += 1;
                self.selection_changed(cx);
            }
        }
    }

    fn session_order(&self) -> Vec<SessionRow> {
        let (needs, recent) = session_rows(&self.sessions, &self.approvals, now_secs());
        needs.into_iter().chain(recent).collect()
    }

    fn len(&self) -> usize {
        self.session_order().len()
    }

    fn clamp(&mut self) {
        self.selected = self.selected.min(self.len().saturating_sub(1));
    }

    fn selected_session(&self) -> Option<String> {
        self.session_order()
            .get(self.selected)
            .map(|r| r.id.clone())
    }

    fn select(&mut self, n: usize, cx: &mut Context<Self>) {
        if n < self.len() && n != self.selected {
            self.selected = n;
            self.scroll.scroll_to_item(n);
            self.selection_changed(cx);
            cx.notify();
        }
    }

    /// Fetches what the selection previews.
    fn selection_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.selected_session() {
            self.fetch_preview(session, cx);
        }
    }

    fn fetch_preview(&mut self, session: String, cx: &mut Context<Self>) {
        let Some(info) = self
            .sessions
            .iter()
            .find(|info| info.id == session)
            .cloned()
        else {
            return;
        };
        if preview_is_current(
            &info,
            self.preview.as_ref().map(|preview| &preview.info),
            self.preview_request.as_ref(),
        ) {
            return;
        }
        self.preview_generation += 1;
        let generation = self.preview_generation;
        self.preview_request = Some(info.clone());
        let connection = self.connection.clone();
        cx.spawn(async move |this, cx| {
            let history = connection
                .request(ClientCommand::SessionHistory {
                    session: session.clone(),
                    before: None,
                    messages: PREVIEW_ITEMS as u32,
                })
                .await;
            let desk = connection
                .request(ClientCommand::DeskGet {
                    session: session.clone(),
                })
                .await;
            let mut chat = Chat::new(&session);
            if let Ok(CommandResult::History { events, more }) = history {
                chat.history_loaded(&events, more);
            }
            let desk = match desk {
                Ok(CommandResult::Desk { desk }) => desk,
                _ => Desk::default(),
            };
            let _ = this.update(cx, |home, cx| {
                if generation != home.preview_generation {
                    return; // A newer revision was requested while this one was loading.
                }
                home.preview_request = None;
                // Still the one selected.
                if home.selected_session().as_ref() == Some(&session) {
                    home.preview = Some(Preview {
                        session,
                        info,
                        chat,
                        desk,
                    });
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Enter: open the session.
    fn activate(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.selected_session() {
            cx.emit(HomeEvent::Open(session));
        }
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if ks.modifiers.control || ks.modifiers.alt || ks.modifiers.platform {
            return;
        }
        match ks.key.as_str() {
            "down" | "j" => self.select(self.selected + 1, cx),
            "up" | "k" => self.select(self.selected.saturating_sub(1), cx),
            "enter" => self.activate(cx),
            "escape" => cx.emit(HomeEvent::Back),
            "n" => cx.emit(HomeEvent::NewSession),
            _ => return,
        }
        cx.stop_propagation();
    }
}

/// Keep the selected session selected when its rows reorder.
fn preserve_session_selection(selected: usize, before: Option<&str>, rows: &[SessionRow]) -> usize {
    before
        .and_then(|id| rows.iter().position(|row| row.id == id))
        .unwrap_or(selected)
}

/// An in-flight request counts as current too, so unrelated approvals do not duplicate it.
fn preview_is_current(
    info: &SessionInfo,
    loaded: Option<&SessionInfo>,
    loading: Option<&SessionInfo>,
) -> bool {
    loaded == Some(info) || loading == Some(info)
}

fn label(text: impl Into<SharedString>, count: Option<usize>, warn: bool) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap(px(8.))
        .pt(px(18.))
        .pb(px(8.))
        .px(px(4.))
        .text_size(px(TEXT_SMALL))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(FG_FAINT))
        .child(text.into())
        .when_some(count, |d, n| {
            d.child(
                div()
                    .text_color(rgb(if warn { WARN } else { FG_FAINT }))
                    .child(n.to_string()),
            )
        })
        .into_any_element()
}

fn side_label(text: &'static str) -> AnyElement {
    div()
        .pt(px(18.))
        .pb(px(7.))
        .text_size(px(TEXT_SMALL))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(FG_FAINT))
        .child(text)
        .into_any_element()
}

/// A bordered list of small rows (the stage tabs).
fn desk_list(rows: Vec<AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .border_1()
        .border_color(rgb(BORDER))
        .rounded(px(R_CARD))
        .overflow_hidden()
        .children(rows.into_iter().enumerate().map(|(n, row)| {
            div()
                .when(n > 0, |d| d.border_t_1().border_color(rgb(BORDER)))
                .child(row)
        }))
        .into_any_element()
}

fn desk_item(
    left: impl Into<SharedString>,
    right: impl Into<SharedString>,
    colour: u32,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap(px(8.))
        .px(px(10.))
        .py(px(7.))
        .text_size(px(TEXT_SIZE))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(rgb(FG))
                .child(left.into()),
        )
        .child(
            div()
                .flex_none()
                .text_color(rgb(colour))
                .child(right.into()),
        )
        .into_any_element()
}

fn empty(text: &'static str) -> AnyElement {
    div()
        .py(px(10.))
        .text_size(px(TEXT_SIZE))
        .text_color(rgb(FG_FAINT))
        .child(text)
        .into_any_element()
}

fn action(id: &'static str, text: &'static str, primary: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px(px(12.))
        .py(px(6.))
        .rounded(px(R_CONTROL))
        .cursor_pointer()
        .text_size(px(TEXT_SIZE))
        .when(primary, |d| {
            d.bg(rgb(ACCENT))
                .text_color(rgb(FG_BRIGHT))
                .hover(|d| d.bg(rgb(ACCENT_HOVER)))
        })
        .when(!primary, |d| {
            d.bg(rgb(SURFACE_2))
                .text_color(rgb(FG))
                .hover(|d| d.bg(rgb(BORDER_2)))
        })
        .child(text)
}

impl HomeView {
    /// A list row: dot, its line and preview, where, how long ago.
    #[allow(clippy::too_many_arguments)]
    fn row(
        &self,
        n: usize,
        dot: u32,
        name: String,
        id: String,
        status: (String, u32),
        preview: String,
        place: String,
        age: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = n == self.selected;
        div()
            .id(("home-row", n))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.))
            .px(px(14.))
            .py(px(10.))
            .rounded(px(R_CONTROL))
            .border_l_2()
            .border_color(rgb(if selected { ACCENT } else { BG }))
            .when(selected, |d| d.bg(rgb(SURFACE_2)))
            .hover(|d| d.bg(rgb(PANEL_2)))
            .cursor_pointer()
            .on_click(cx.listener(move |home, ev: &gpui::ClickEvent, _, cx| {
                if ev.click_count() > 1 || home.selected == n {
                    home.selected = n;
                    home.activate(cx);
                } else {
                    home.select(n, cx);
                }
            }))
            .child(
                div()
                    .flex_none()
                    .size(px(7.))
                    .rounded(px(R_DOT))
                    .bg(rgb(dot)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(8.))
                            .text_size(px(TEXT_SIZE))
                            .child(div().text_color(rgb(FG_BRIGHT)).child(name))
                            .child(div().text_color(rgb(FG_FAINT)).child(id))
                            .child(div().text_color(rgb(status.1)).child(status.0)),
                    )
                    .child(
                        div()
                            .font_family(PROSE)
                            .text_size(px(TEXT_SIZE))
                            .text_color(rgb(FG_DIM))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(preview),
                    ),
            )
            .child(
                div()
                    .flex_shrink(1.)
                    .max_w(px(320.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(TEXT_SMALL))
                    .text_color(rgb(FG_FAINT))
                    .child(place),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(48.))
                    .flex()
                    .justify_end()
                    .text_size(px(TEXT_SMALL))
                    .text_color(rgb(FG_FAINT))
                    .child(age),
            )
            .child(div().flex_none().text_color(rgb(FG_FAINT)).child("›"))
            .into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        let (needs, recent) = session_rows(&self.sessions, &self.approvals, now_secs());
        let mut n = 0;
        for (title, rows) in [("NEEDS YOU", needs), ("RECENT", recent)] {
            if rows.is_empty() {
                continue;
            }
            out.push(label(title, Some(rows.len()), title == "NEEDS YOU"));
            for r in rows {
                let dot = if r.waiting {
                    WARN
                } else if r.status == "idle" {
                    FG_FAINT
                } else {
                    OK
                };
                let colour = if r.waiting { WARN } else { FG_FAINT };
                out.push(self.row(
                    n,
                    dot,
                    r.kind,
                    format!("#{}", r.id),
                    (r.status, colour),
                    r.preview,
                    r.place,
                    r.age,
                    cx,
                ));
                n += 1;
            }
        }
        if n == 0 {
            out.push(empty("No sessions yet: n starts one."));
        }
        out
    }

    fn session_side(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(id) = self.selected_session() else {
            return vec![];
        };
        let Some(info) = self.sessions.iter().find(|s| s.id == id) else {
            return vec![];
        };
        let mut out = vec![
            div()
                .flex()
                .flex_row()
                .gap(px(8.))
                .text_size(px(TEXT_SIZE))
                .child(
                    div()
                        .text_color(rgb(FG_BRIGHT))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(kind_label(info)),
                )
                .child(
                    div()
                        .text_color(rgb(FG_FAINT))
                        .child(format!("#{}", info.id)),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(TEXT_SMALL))
                        .text_color(rgb(FG_FAINT))
                        .child(state_word(info.state)),
                )
                .into_any_element(),
            context_line(&[
                ("site", info.site.clone().unwrap_or_else(|| "–".into())),
                (
                    "model",
                    info.model.clone().unwrap_or_else(|| "default".into()),
                ),
            ]),
        ];
        let waiting: Vec<&Approval> = self
            .approvals
            .values()
            .filter(|a| a.session == id)
            .collect();
        if !waiting.is_empty() {
            out.push(side_label("WAITING"));
            for a in waiting {
                let answer = |decision: ApprovalDecision| {
                    let request = a.request.clone();
                    cx.listener(move |_, _, _, cx| {
                        cx.emit(HomeEvent::Answer {
                            request: request.clone(),
                            decision,
                        })
                    })
                };
                out.push(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(9.))
                        .px(px(10.))
                        .py(px(7.))
                        .border_1()
                        .border_color(rgb(ACCENT_BORDER))
                        .bg(rgb(ACCENT_DEEP))
                        .rounded(px(R_CARD))
                        .text_size(px(TEXT_SMALL))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_color(rgb(ACCENT_SOFT))
                                .child(format!("⚑ {} {}", asked_line(a), a.args.join(" "))),
                        )
                        .child(
                            action_small(format!("home-deny-{}", a.request), "Deny", false)
                                .on_click(answer(ApprovalDecision::Deny)),
                        )
                        .child(
                            action_small(format!("home-allow-{}", a.request), "Allow", true)
                                .on_click(answer(ApprovalDecision::Approve)),
                        )
                        .into_any_element(),
                );
            }
        }
        let preview = self.preview.as_ref().filter(|p| p.session == id);
        out.push(side_label("LAST"));
        match preview {
            None => out.push(empty("loading…")),
            Some(p) => {
                let said: Vec<(bool, &str)> = p
                    .chat
                    .items
                    .iter()
                    .filter_map(|i| match i {
                        ChatItem::User { text, .. } => Some((true, text.as_str())),
                        ChatItem::Text { text } => Some((false, text.as_str())),
                        _ => None,
                    })
                    .collect();
                if said.is_empty() {
                    out.push(empty("Nothing said yet."));
                }
                for (you, text) in said.iter().rev().take(3).rev() {
                    // Fences left out: the preview is prose.
                    let prose: String = text
                        .lines()
                        .filter(|l| !l.trim_start().starts_with("```"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let mut short: String = prose.chars().take(280).collect();
                    if prose.chars().count() > 280 {
                        short.push('…');
                    }
                    out.push(
                        div()
                            .pl(px(12.))
                            .mb(px(8.))
                            .border_l_2()
                            .border_color(rgb(if *you { OK_TEXT } else { ACCENT_BORDER }))
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .text_size(px(TEXT_SMALL))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(if *you { OK } else { ACCENT_SOFT }))
                                    .child(if *you {
                                        "you".to_string()
                                    } else {
                                        "agent".to_string()
                                    }),
                            )
                            .child(
                                div()
                                    .font_family(PROSE)
                                    .text_size(px(TEXT_SIZE))
                                    .line_height(px(22.))
                                    .text_color(rgb(FG))
                                    .child(short),
                            )
                            .into_any_element(),
                    );
                }
                out.push(side_label("STAGE"));
                if p.desk.tabs.is_empty() {
                    out.push(empty("Nothing on its stage."));
                } else {
                    out.push(desk_list(
                        p.desk
                            .tabs
                            .iter()
                            .map(|e| {
                                desk_item(
                                    stage_tabs::label(&e.tab, info.site.as_deref(), &self.apps),
                                    if e.unseen { "new" } else { "" },
                                    ACCENT_SOFT,
                                )
                            })
                            .collect(),
                    ));
                }
            }
        }
        out.push(
            div()
                .pt(px(18.))
                .flex()
                .flex_row()
                .child(
                    action("open-session", "Open session  ⏎", true).on_click(
                        cx.listener(move |_, _, _, cx| cx.emit(HomeEvent::Open(id.clone()))),
                    ),
                )
                .into_any_element(),
        );
        out
    }
}

fn action_small(id: String, text: &'static str, primary: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(gpui::ElementId::Name(id.into()))
        .px(px(9.))
        .py(px(3.))
        .rounded(px(R_CONTROL))
        .cursor_pointer()
        .when(primary, |d| {
            d.bg(rgb(ACCENT))
                .text_color(rgb(FG_BRIGHT))
                .hover(|d| d.bg(rgb(ACCENT_HOVER)))
        })
        .when(!primary, |d| {
            d.bg(rgb(SURFACE_2))
                .text_color(rgb(FG))
                .hover(|d| d.bg(rgb(BORDER_2)))
        })
        .child(text)
}

/// `site 3d · model m1 · …`: names dim, values bright.
fn context_line(parts: &[(&'static str, String)]) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(14.))
        .pt(px(8.))
        .text_size(px(TEXT_SMALL))
        .children(parts.iter().map(|(name, value)| {
            div()
                .flex()
                .flex_row()
                .gap(px(5.))
                .child(div().text_color(rgb(FG_FAINT)).child(*name))
                .child(div().text_color(rgb(FG)).child(value.clone()))
        }))
        .into_any_element()
}

impl Render for HomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = self.list(cx);
        let side = self.session_side(cx);
        let sessions = self.sessions.len();
        div()
            .id("home")
            .track_focus(&self.focus)
            .key_context("home")
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_row()
            .bg(rgb(BG))
            .text_color(rgb(FG))
            .text_size(px(TEXT_SIZE))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex_none()
                            .px(px(28.))
                            .pt(px(22.))
                            .flex()
                            .flex_col()
                            .gap(px(16.))
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(div().size(px(8.)).rounded(px(R_DOT)).bg(rgb(ACCENT)))
                                    .child(
                                        div()
                                            .text_size(px(16.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(FG_BRIGHT))
                                            .child("home"),
                                    )
                                    .child(div().flex_1())
                                    .child(
                                        action("home-new", "＋ New session  n", false).on_click(
                                            cx.listener(|_, _, _, cx| {
                                                cx.emit(HomeEvent::NewSession)
                                            }),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap(px(22.))
                                    .text_size(px(TEXT_SIZE))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap(px(6.))
                                            .pb(px(8.))
                                            .border_b_2()
                                            .border_color(rgb(ACCENT))
                                            .text_color(rgb(FG_BRIGHT))
                                            .child("sessions")
                                            .child(
                                                div()
                                                    .text_color(rgb(FG_FAINT))
                                                    .child(sessions.to_string()),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("home-list")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .px(px(20.))
                            .pb(px(20.))
                            .children(list),
                    ),
            )
            .child(
                div()
                    .id("home-side")
                    .flex_none()
                    .w(px(440.))
                    .h_full()
                    .overflow_y_scroll()
                    .bg(rgb(PANEL))
                    .border_l_1()
                    .border_color(rgb(BORDER))
                    .px(px(22.))
                    .py(px(20.))
                    .children(side),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, updated: &str) -> SessionInfo {
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": "agentic", "parent": null,
            "state": "idle", "last_seq": 1, "preview": "old", "updated": updated
        }))
        .unwrap()
    }

    #[test]
    fn session_updates_follow_the_selected_session() {
        let sessions = [
            session("b", "2026-11-15T12:00:00Z"),
            session("a", "2026-11-15T11:00:00Z"),
        ];
        let (needs, recent) = session_rows(&sessions, &BTreeMap::new(), 0);
        let rows: Vec<_> = needs.into_iter().chain(recent).collect();
        // Previously b was at index 1; a turn has now moved it to index 0.
        assert_eq!(preserve_session_selection(1, Some("b"), &rows), 0);
        assert_eq!(preserve_session_selection(1, Some("gone"), &rows), 1);
    }

    #[test]
    fn selected_preview_is_refetched_for_a_new_revision_but_not_unrelated_updates() {
        let old = session("a", "2026-11-15T11:00:00Z");
        let mut current = old.clone();
        current.last_seq += 1;
        current.preview = Some("new response".into());
        assert!(preview_is_current(&old, Some(&old), None));
        assert!(!preview_is_current(&current, Some(&old), None));
        assert!(preview_is_current(&current, Some(&old), Some(&current)));
        assert!(!preview_is_current(&current, None, Some(&old)));
        assert!(!preview_is_current(
            &session("b", &old.updated),
            Some(&old),
            None
        ));
    }
}
