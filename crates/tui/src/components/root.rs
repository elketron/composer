use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::{Frame, Terminal};

use crate::action::Action;
use crate::components::{
    ApprovalPopup, ChangesView, ChatPane, CommandLine, Component, Help, PageViewer, Picker,
    ProjectView, Selection, StatusBar,
};
use crate::store::AppState;
use crate::store::types::{FlashLevel, Mode, Pane};
use crate::theme;

/// Below this width only the focused pane is shown.
const TWO_PANE_MIN_WIDTH: u16 = 100;

/// Lines a notch of the mouse wheel scrolls.
const WHEEL_LINES: usize = 3;

/// Layout: two panes over a status bar; the picker floats on top.
pub struct Root {
    pub chat: ChatPane,
    pub page: PageViewer,
    changes: ChangesView,
    project: ProjectView,
    status: StatusBar,
    command: CommandLine,
    picker: Picker,
    approval: ApprovalPopup,
    help: Help,
    /// Text selected with the mouse (shown until the next click or key).
    pub selection: Option<Selection>,
}

impl Root {
    /// `bindings`: the keymap's keys and descriptions, for help.
    pub fn new(bindings: Vec<(String, &'static str)>) -> Self {
        Self {
            chat: ChatPane::new(),
            page: PageViewer::new(),
            changes: ChangesView::new(),
            project: ProjectView::new(),
            status: StatusBar,
            command: CommandLine::new(),
            picker: Picker,
            approval: ApprovalPopup,
            help: Help::new(bindings),
            selection: None,
        }
    }

    /// The mouse: a drag selects text in the pane it starts in and copies it on release, a click
    /// focuses the pane, the wheel scrolls the pane under it. `screen`: the whole terminal.
    pub fn handle_mouse(
        &mut self,
        event: MouseEvent,
        screen: Rect,
        state: &AppState,
    ) -> Vec<Action> {
        if state.ui.picker_open || state.ui.help_open || state.approvals.current().is_some() {
            return vec![];
        }
        let at = Position::new(event.column, event.row);
        let [panes, _] = Self::split(screen);
        let under = pane_areas(panes, state)
            .into_iter()
            .find(|(_, area)| area.contains(at));
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Inside the pane's border.
                self.selection =
                    under.map(|(_, area)| Selection::start(area.inner(Margin::new(1, 1)), at));
                vec![]
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(selection) = &mut self.selection {
                    selection.extend(at);
                }
                vec![]
            }
            MouseEventKind::Up(MouseButton::Left) => match self.selection {
                Some(selection) if !selection.is_empty() => {
                    let text = self.selected_text(selection, screen, state);
                    if text.is_empty() {
                        vec![]
                    } else {
                        vec![Action::CopyText(text)]
                    }
                }
                _ => {
                    self.selection = None;
                    under
                        .map(|(pane, _)| Action::FocusPane(pane))
                        .into_iter()
                        .collect()
                }
            },
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => match under {
                Some((pane, _)) => self.scroll(pane, event.kind == MouseEventKind::ScrollUp, state),
                None => vec![],
            },
            _ => vec![],
        }
    }

    /// What the selection covers on screen, from a fresh render of it.
    fn selected_text(&self, selection: Selection, screen: Rect, state: &AppState) -> String {
        let Ok(mut terminal) = Terminal::new(TestBackend::new(screen.width, screen.height));
        let Ok(_) = terminal.draw(|frame| self.render(frame, frame.area(), state));
        selection.text(terminal.backend().buffer())
    }

    fn scroll(&mut self, pane: Pane, up: bool, state: &AppState) -> Vec<Action> {
        match pane {
            Pane::Chat => return self.chat.scroll_by(up, WHEEL_LINES, state),
            Pane::Page if state.project_view.open || state.project.open => {
                let key = KeyEvent::new(
                    if up { KeyCode::Up } else { KeyCode::Down },
                    KeyModifiers::NONE,
                );
                for _ in 0..WHEEL_LINES {
                    if state.project_view.open {
                        self.project.handle_key(key, state);
                    } else {
                        self.changes.handle_key(key, state);
                    }
                }
            }
            Pane::Page => self.page.scroll_by(up, WHEEL_LINES),
        }
        vec![]
    }

    /// The panes over the status bar.
    fn split(area: Rect) -> [Rect; 2] {
        Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(area)
    }

    /// The right pane: the project view or the changes view when open, else the page viewer.
    fn render_right(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if state.project_view.open {
            self.project.render(frame, area, state);
        } else if state.project.open {
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
                Pane::Page if state.project_view.open => self.project.handle_key(key, state),
                Pane::Page if state.project.open => self.changes.handle_key(key, state),
                Pane::Page => self.page.handle_key(key, state),
            },
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        frame.render_widget(Block::new().style(Style::new().bg(theme::BG)), area);
        let [panes, status] = Self::split(area);
        for (pane, pane_area) in pane_areas(panes, state) {
            match pane {
                Pane::Chat => self.chat.render(frame, pane_area, state),
                Pane::Page => self.render_right(frame, pane_area, state),
            }
        }
        if let Some(selection) = &self.selection {
            selection.highlight(frame.buffer_mut());
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

/// Where the panes go: side by side, or only one (maximized, or too narrow for two).
fn pane_areas(panes: Rect, state: &AppState) -> Vec<(Pane, Rect)> {
    let single = state
        .ui
        .maximized
        .or_else(|| (panes.width < TWO_PANE_MIN_WIDTH).then_some(state.ui.focus));
    match single {
        Some(pane) => vec![(pane, panes)],
        None => {
            let [chat, _, page] = Layout::horizontal([
                Constraint::Percentage(54),
                Constraint::Length(1),
                Constraint::Min(30),
            ])
            .areas(panes);
            vec![(Pane::Chat, chat), (Pane::Page, page)]
        }
    }
}
