use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::action::Action;
use crate::components::{
    ApprovalPopup, ChatPane, CommandLine, Component, PageViewer, Picker, StatusBar,
};
use crate::store::AppState;
use crate::store::types::{Mode, Pane};
use crate::theme;

/// Below this width only the focused pane is shown.
const TWO_PANE_MIN_WIDTH: u16 = 100;

/// Layout: two panes over a status bar; the picker floats on top.
pub struct Root {
    pub chat: ChatPane,
    pub page: PageViewer,
    status: StatusBar,
    command: CommandLine,
    picker: Picker,
    approval: ApprovalPopup,
}

impl Root {
    pub fn new() -> Self {
        Self {
            chat: ChatPane::new(),
            page: PageViewer::new(),
            status: StatusBar,
            command: CommandLine::new(),
            picker: Picker,
            approval: ApprovalPopup,
        }
    }
}

impl Component for Root {
    /// Routes a key the keymap didn't take: overlay, then the mode's owner, then the focused pane.
    fn handle_key(&mut self, key: KeyEvent, state: &AppState) -> Vec<Action> {
        if state.approvals.current().is_some() {
            return self.approval.handle_key(key, state);
        }
        if state.ui.picker_open {
            return self.picker.handle_key(key, state);
        }
        // Normal-mode Enter sends the draft, whichever pane has focus.
        if state.mode() == Mode::Normal
            && key.code == KeyCode::Enter
            && let Some(text) = self.chat.composer.take_draft()
        {
            return vec![Action::SendMessage(text)];
        }
        match state.mode() {
            Mode::Insert => self.chat.handle_key(key, state),
            Mode::Command => self.command.handle_key(key, state),
            Mode::Hint => self.page.handle_key(key, state),
            Mode::Normal => match state.ui.focus {
                Pane::Chat => self.chat.handle_key(key, state),
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
            Some(Pane::Page) => self.page.render(frame, panes, state),
            None => {
                let [chat, _, page] = Layout::horizontal([
                    Constraint::Percentage(54),
                    Constraint::Length(1),
                    Constraint::Min(30),
                ])
                .areas(panes);
                self.chat.render(frame, chat, state);
                self.page.render(frame, page, state);
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
        if state.approvals.current().is_some() {
            self.approval.render(frame, area, state);
        }
    }
}
