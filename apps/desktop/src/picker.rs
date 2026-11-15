//! A list to pick from by typing (the palette, the workspace picker, the session switcher): a
//! query line, the matching items best first, ↑/↓ (or ctrl+p/ctrl+n) to move, Enter to pick,
//! Escape to close.

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, FontWeight, KeyDownEvent, Render, Window,
    div, prelude::*, px, rgb,
};

use crate::matcher::ranked;
use crate::theme::*;

/// What picking an item does (the window carries it out).
#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    Workspace(String),
    Session(String),
    /// A new session: asks for its project (it always runs as the orchestrator).
    NewSession,
    /// Start the new session in this project (a folder) or none.
    NewSessionIn(Option<String>),
    Page(String),
    App(String),
    /// A `[[launcher]]` entry (it runs on this machine: the server's, on Linux).
    Launch(String),
    AttachFile,
    NewTerminal,
    ToggleFollow,
    /// Enter this site (`site.set`) for the shown session.
    Site(String),
    /// Leave the shown session's site.
    LeaveSite,
    Workspaces,
    Sessions,
    /// Pick the shown session's project, or its model; compact it.
    ProjectMenu,
    ModelMenu,
    Compact,
    /// The shown session's project (a folder) or none; its model or the server's.
    SetProject(Option<String>),
    SetModel(Option<String>),
    /// The workspace home: its sessions, or its projects.
    Home,
    Projects,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub label: String,
    /// At the right: a key, a site and project, what the command does.
    pub detail: String,
    /// A second line under the label (a session's last words); empty for most.
    pub note: String,
    /// The palette's section (session, site, stage, workspace, sessions…): headed while the query
    /// is empty, items of a group kept together.
    pub group: &'static str,
    pub pick: Pick,
}

pub enum PickerEvent {
    Picked(Pick),
    Dismissed,
}

pub struct Picker {
    title: &'static str,
    items: Vec<Item>,
    query: String,
    shown: Vec<usize>,
    selected: usize,
    focus: FocusHandle,
}

/// Rows shown at once.
const ROWS: usize = 12;

/// The palette's sections, in the order they are shown while nothing is typed.
const GROUPS: [&str; 8] = [
    "session",
    "site",
    "stage",
    "launch",
    "pages",
    "workspace",
    "sessions",
    "",
];

impl EventEmitter<PickerEvent> for Picker {}

impl Picker {
    pub fn new(title: &'static str, items: Vec<Item>, cx: &mut Context<Self>) -> Self {
        let mut picker = Self {
            title,
            items,
            query: String::new(),
            shown: Vec::new(),
            selected: 0,
            focus: cx.focus_handle(),
        };
        picker.refilter();
        picker
    }

    /// More items (ones fetched after it opened).
    pub fn extend(&mut self, items: Vec<Item>, cx: &mut Context<Self>) {
        self.items.extend(items);
        self.refilter();
        cx.notify();
    }

    fn refilter(&mut self) {
        self.shown = ranked(&self.query, self.items.iter().map(|i| i.label.as_str()));
        // Nothing typed: grouped, each group in the order it came. Typed: best match first.
        if self.query.is_empty() {
            let rank = |group: &str| {
                GROUPS
                    .iter()
                    .position(|g| *g == group)
                    .unwrap_or(GROUPS.len())
            };
            self.shown.sort_by_key(|&i| rank(self.items[i].group));
        }
        self.selected = self.selected.min(self.shown.len().saturating_sub(1));
    }

    /// More than one group: the rows get section heads and a category column.
    fn grouped(&self) -> bool {
        let mut groups = self.items.iter().map(|i| i.group);
        let first = groups.next();
        groups.any(|g| Some(g) != first)
    }

    /// The panel's head, the search's placeholder, and what the footer counts.
    fn words(&self) -> (&'static str, &'static str, &'static str) {
        match self.title {
            "session" => ("Switch session", "Find a session", "session"),
            "workspace" => ("Switch workspace", "Find a workspace", "workspace"),
            "project" => ("Set the session's project", "Find a project", "project"),
            "model" => ("The session's model", "Find a model", "model"),
            _ => ("Commands", "Type a command", "command"),
        }
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        match ks.key.as_str() {
            "escape" => cx.emit(PickerEvent::Dismissed),
            "enter" => {
                if let Some(item) = self.shown.get(self.selected).map(|&i| &self.items[i]) {
                    cx.emit(PickerEvent::Picked(item.pick.clone()));
                }
            }
            "up" => self.selected = self.selected.saturating_sub(1),
            "p" if m.control => self.selected = self.selected.saturating_sub(1),
            "down" => self.selected = (self.selected + 1).min(self.shown.len().saturating_sub(1)),
            "n" if m.control => {
                self.selected = (self.selected + 1).min(self.shown.len().saturating_sub(1))
            }
            "backspace" => {
                self.query.pop();
                self.selected = 0;
                self.refilter();
            }
            _ if !m.control && !m.alt && !m.platform => {
                let Some(text) = ks.key_char.as_deref() else {
                    return;
                };
                self.query.push_str(text);
                self.selected = 0;
                self.refilter();
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Focusable for Picker {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Picker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (head, placeholder, noun) = self.words();
        let grouped = self.grouped();
        let headings = grouped && self.query.is_empty();
        let first = self.selected.saturating_sub(ROWS - 1);
        let mut rows = Vec::new();
        let mut last_group = None;
        for (at, &i) in self.shown.iter().enumerate().skip(first).take(ROWS) {
            let item = &self.items[i];
            if headings && last_group != Some(item.group) && !item.group.is_empty() {
                rows.push(
                    div()
                        .px(px(13.))
                        .pt(px(10.))
                        .pb(px(6.))
                        .text_size(px(TEXT_SMALL))
                        .text_color(rgb(FG_FAINT))
                        .child(item.group.to_uppercase())
                        .into_any_element(),
                );
            }
            last_group = Some(item.group);
            let selected = at == self.selected;
            let pick = item.pick.clone();
            rows.push(
                div()
                    .id(at)
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(12.))
                    .px(px(13.))
                    .py(px(if item.note.is_empty() { 8. } else { 10. }))
                    .border_l_2()
                    .border_color(rgb(if selected { ACCENT } else { PANEL }))
                    .when(selected, |d| d.bg(rgb(SURFACE_2)))
                    .hover(|d| d.bg(rgb(PANEL_2)))
                    .on_click(
                        cx.listener(move |_, _, _, cx| cx.emit(PickerEvent::Picked(pick.clone()))),
                    )
                    .when(grouped && !headings, |d| {
                        d.child(
                            div()
                                .w(px(64.))
                                .flex_none()
                                .text_size(px(TEXT_SMALL))
                                .text_color(rgb(FG_FAINT))
                                .child(item.group),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(120.))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(div().text_color(rgb(FG_BRIGHT)).child(label(&item.label)))
                            .when(!item.note.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_size(px(TEXT_SMALL))
                                        .text_color(rgb(FG_DIM))
                                        .child(item.note.clone()),
                                )
                            }),
                    )
                    .child(
                        // The detail gives way first: a long path ends in an ellipsis.
                        div()
                            .flex_shrink(1.)
                            .min_w(px(0.))
                            .max_w(px(260.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(TEXT_SMALL))
                            .text_color(rgb(FG_FAINT))
                            .child(item.detail.clone()),
                    )
                    .into_any_element(),
            );
        }
        let key = |text: &'static str| {
            div()
                .px(px(4.))
                .border_1()
                .border_color(rgb(BORDER_2))
                .rounded(px(R_SM))
                .text_color(rgb(FG_DIM))
                .child(text)
        };
        let count = self.shown.len();
        div()
            .track_focus(&self.focus)
            .key_context("picker")
            .on_key_down(cx.listener(Self::on_key_down))
            .w_full()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER_2))
            .rounded(px(R_CARD))
            .overflow_hidden()
            .shadow_lg()
            .text_size(px(TEXT_SIZE))
            .text_color(rgb(FG))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(px(13.))
                    .py(px(9.))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_color(rgb(FG_DIM))
                    .child(head)
                    .child(key("esc").text_size(px(TEXT_SMALL))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(14.))
                    .px(px(13.))
                    .py(px(14.))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(TEXT_SIZE))
                    .child(div().text_color(rgb(FG_FAINT)).child(if noun == "command" {
                        "›"
                    } else {
                        "⌕"
                    }))
                    .child(if self.query.is_empty() {
                        div()
                            .text_color(rgb(FG_FAINT))
                            .child(format!("▏{placeholder}"))
                    } else {
                        div()
                            .text_color(rgb(FG_BRIGHT))
                            .child(format!("{}▏", self.query))
                    }),
            )
            .child(
                div()
                    .py(px(4.))
                    .children(rows)
                    .when(self.shown.is_empty(), |d| {
                        d.child(
                            div()
                                .px(px(13.))
                                .py(px(10.))
                                .text_color(rgb(FG_DIM))
                                .child("Nothing matches."),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.))
                    .px(px(13.))
                    .py(px(8.))
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(TEXT_SMALL))
                    .text_color(rgb(FG_FAINT))
                    .child("↑↓ move")
                    .child(format!(
                        "Enter {}",
                        if noun == "command" { "run" } else { "open" }
                    ))
                    .child("Esc close")
                    .child(div().flex_1())
                    .child(format!(
                        "{count} {noun}{}",
                        if count == 1 { "" } else { "s" }
                    )),
            )
    }
}

/// A label with its marks coloured: "· shown" in the accent, "! waiting" in yellow.
fn label(text: &str) -> gpui::AnyElement {
    let (name, mark, colour) = if let Some(at) = text.find(" · shown") {
        (&text[..at], &text[at..], ACCENT_SOFT)
    } else if let Some(at) = text.find(" ! waiting") {
        (&text[..at], &text[at..], WARN)
    } else {
        return div().child(text.to_string()).into_any_element();
    };
    div()
        .flex()
        .flex_row()
        .gap(px(6.))
        .child(name.to_string())
        .child(
            div()
                .text_color(rgb(colour))
                .font_weight(FontWeight::NORMAL)
                .child(mark.trim().to_string()),
        )
        .into_any_element()
}
