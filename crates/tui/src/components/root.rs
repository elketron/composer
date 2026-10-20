use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::action::Action;
use crate::components::{
    ApprovalPopup, ChangesView, ChatPane, CommandLine, Component, Help, PageViewer, Picker,
    StatusBar,
};
use crate::store::AppState;
use crate::store::types::{FlashLevel, Mode, Pane};
use crate::theme;

/// Below this width only the focused pane is shown.
const TWO_PANE_MIN_WIDTH: u16 = 100;

/// Layout: two panes over a status bar; the picker floats on top.
pub struct Root {
    pub chat: ChatPane,
    pub page: PageViewer,
    changes: ChangesView,
    status: StatusBar,
    command: CommandLine,
    picker: Picker,
    approval: ApprovalPopup,
    help: Help,
}

impl Root {
    /// `bindings`: the keymap's keys and descriptions, for help.
    pub fn new(bindings: Vec<(String, &'static str)>) -> Self {
        Self {
            chat: ChatPane::new(),
            page: PageViewer::new(),
            changes: ChangesView::new(),
            status: StatusBar,
            command: CommandLine::new(),
            picker: Picker,
            approval: ApprovalPopup,
            help: Help::new(bindings),
        }
    }

    /// The right pane: the changes view when open, else the page viewer.
    fn render_right(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if state.project.open {
            self.changes.render(frame, area, state);
        } else {
            self.page.render(frame, area, state);
        }
    }

    /// Pasted text (bracketed paste): into the message while writing; otherwise ignored.
    pub fn handle_paste(&mut self, text: &str, state: &AppState) -> Vec<Action> {
        if state.mode() == Mode::Insert {
            self.chat.composer.paste(text)
        } else {
            vec![]
        }
    }
}

impl Component for Root {
    /// Routes a key the keymap didn't take: overlay, then the mode's owner, then the focused pane.
    fn handle_key(&mut self, key: KeyEvent, state: &AppState) -> Vec<Action> {
        if state.approvals.current().is_some() {
            return self.approval.handle_key(key, state);
        }
        if state.ui.help_open {
            return self.help.handle_key(key, state);
        }
        if state.ui.picker_open {
            return self.picker.handle_key(key, state);
        }
        // Normal-mode Enter sends the draft (and attached files), whichever pane has focus.
        // Not while a file is still uploading: the draft stays.
        if state.mode() == Mode::Normal && key.code == KeyCode::Enter {
            let ready = self.chat.composer.has_draft() || state.attachments.has_ready();
            if ready && let Some(why) = state.attachments.blocking() {
                return vec![Action::Flash(
                    format!("not sent yet: {why}"),
                    FlashLevel::Warn,
                )];
            }
            if let Some(text) = self.chat.composer.take_draft() {
                return vec![Action::SendMessage(text)];
            }
            if state.attachments.has_ready() {
                return vec![Action::SendMessage(String::new())];
            }
        }
        match state.mode() {
            Mode::Insert => self.chat.handle_key(key, state),
            Mode::Command => self.command.handle_key(key, state),
            Mode::Hint => self.page.handle_key(key, state),
            Mode::Normal => match state.ui.focus {
                Pane::Chat => self.chat.handle_key(key, state),
                Pane::Page if state.project.open => self.changes.handle_key(key, state),
                Pane::Page => self.page.handle_key(key, state),
            },
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        frame.render_widget(Block::new().style(Style::new().bg(theme::BG)), area);
        let [panes, status] =
            Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(area);

        let single = state
            .ui
            .maximized
            .or_else(|| (panes.width < TWO_PANE_MIN_WIDTH).then_some(state.ui.focus));
        match single {
            Some(Pane::Chat) => self.chat.render(frame, panes, state),
            Some(Pane::Page) => self.render_right(frame, panes, state),
            None => {
                let [chat, _, page] = Layout::horizontal([
                    Constraint::Percentage(54),
                    Constraint::Length(1),
                    Constraint::Min(30),
                ])
                .areas(panes);
                self.chat.render(frame, chat, state);
                self.render_right(frame, page, state);
            }
        }

        if state.mode() == Mode::Command {
            self.command.render(frame, status, state);
        } else {
            self.status.render(frame, status, state);
        }
        if state.ui.picker_open {
            self.picker.render(frame, area, state);
        }
        if state.ui.help_open {
            self.help.render(frame, area, state);
        }
        if state.approvals.current().is_some() {
            self.approval.render(frame, area, state);
        }
    }
}
