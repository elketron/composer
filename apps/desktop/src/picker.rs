//! A list to pick from by typing (the palette, the workspace picker, the session switcher): a
//! query line, the matching items best first, ↑/↓ (or ctrl+p/ctrl+n) to move, Enter to pick,
//! Escape to close.

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, Render, Window, div,
    prelude::*, px, rgb,
};

use crate::matcher::ranked;

/// What picking an item does (the window carries it out).
#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    Workspace(String),
    Session(String),
    Page(String),
    App(String),
    /// A `[[launcher]]` entry (it runs on this machine: the server's, on Linux).
    Launch(String),
    AttachFile,
    NewTerminal,
    ToggleFollow,
    Workspaces,
    Sessions,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub label: String,
    pub detail: String,
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
const ROWS: usize = 14;

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
        self.selected = self.selected.min(self.shown.len().saturating_sub(1));
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
        let first = self.selected.saturating_sub(ROWS - 1);
        let rows = self
            .shown
            .iter()
            .enumerate()
            .skip(first)
            .take(ROWS)
            .map(|(at, &i)| {
                let item = &self.items[i];
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .gap_4()
                    .px_3()
                    .py_1()
                    .when(at == self.selected, |d| d.bg(rgb(0x2a2a36)))
                    .child(div().text_color(rgb(0xe8e8ee)).child(item.label.clone()))
                    .child(div().text_color(rgb(0x85858f)).child(item.detail.clone()))
            });
        div()
            .track_focus(&self.focus)
            .key_context("picker")
            .on_key_down(cx.listener(Self::on_key_down))
            .w(px(620.))
            .bg(rgb(0x15151b))
            .border_1()
            .border_color(rgb(0x3a3a48))
            .rounded_md()
            .shadow_lg()
            .text_size(px(13.))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0x2a2a36))
                    .text_color(rgb(0xe8e8ee))
                    .child(format!("{}  › {}▏", self.title, self.query)),
            )
            .children(rows)
            .when(self.shown.is_empty(), |d| {
                d.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_color(rgb(0x85858f))
                        .child("nothing matches"),
                )
            })
    }
}
