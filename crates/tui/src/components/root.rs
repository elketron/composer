use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::{Frame, Terminal};

use crate::action::Action;
use crate::components::{
    ApprovalPopup, ChangesView, ChatPane, CommandLine, Component, Help, PageViewer, Picker,
    ProjectView, Selection, StatusBar, TabBar,
};
use crate::store::AppState;
use crate::store::StageTab;
use crate::store::types::{FlashLevel, Mode, Pane};
use crate::theme;

/// Below this width only the focused pane is shown.
const TWO_PANE_MIN_WIDTH: u16 = 100;

/// Lines a notch of the mouse wheel scrolls.
const WHEEL_LINES: usize = 3;

/// Layout: the chat and the stage over a status bar; the picker floats on top.
pub struct Root {
    pub chat: ChatPane,
    pub page: PageViewer,
    changes: ChangesView,
    project: ProjectView,
    tabs: TabBar,
    status: StatusBar,
    command: CommandLine,
    picker: Picker,
    approval: ApprovalPopup,
    help: Help,
    /// Text selected with the mouse (shown until the next click or key).
    pub selection: Option<Selection>,
    /// A click that landed on a tab, to be turned into an action on release.
    tab_click: Option<usize>,
}

impl Root {
    /// `bindings`: the keymap's keys and descriptions, for help.
    pub fn new(bindings: Vec<(String, &'static str)>) -> Self {
        Self {
            chat: ChatPane::new(),
            page: PageViewer::new(),
            changes: ChangesView::new(),
            project: ProjectView::new(),
            tabs: TabBar,
            status: StatusBar,
            command: CommandLine::new(),
            picker: Picker,
            approval: ApprovalPopup,
            help: Help::new(bindings),
            selection: None,
            tab_click: None,
        }
    }

    /// The mouse: a drag selects text in the pane it starts in and copies it on release, a click
    /// focuses the pane or picks a tab, the wheel scrolls the pane under it. `screen`: the whole
    /// terminal.
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
                self.tab_click = under
                    .filter(|(pane, _)| *pane == Pane::Stage)
                    .and_then(|(_, area)| self.tabs.hit(state, area, at));
                if self.tab_click.is_some() {
                    return vec![];
                }
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
            MouseEventKind::Up(MouseButton::Left) => {
                match self.selection {
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
                        // A click that started on a tab picks that tab, and takes the focus
                        // with it: clicking a tab means wanting to read it.
                        match self.tab_click.take() {
                            Some(index) => {
                                vec![Action::FocusPane(Pane::Stage), Action::ShowTab(index)]
                            }
                            None => under
                                .map(|(pane, _)| Action::FocusPane(pane))
                                .into_iter()
                                .collect(),
                        }
                    }
                }
            }
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
        let key = KeyEvent::new(
            if up { KeyCode::Up } else { KeyCode::Down },
            KeyModifiers::NONE,
        );
        match pane {
            Pane::Chat => return self.chat.scroll_by(up, WHEEL_LINES, state),
            Pane::Stage => match state.shown_tab() {
                Some(StageTab::Project { .. }) => {
                    for _ in 0..WHEEL_LINES {
                        self.project.handle_key(key, state);
                    }
                }
                Some(StageTab::Changes { .. }) => {
                    for _ in 0..WHEEL_LINES {
                        self.changes.handle_key(key, state);
                    }
                }
                _ => self.page.scroll_by(up, WHEEL_LINES),
            },
        }
        vec![]
    }

    /// The panes over the status bar.
    fn split(area: Rect) -> [Rect; 2] {
        Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(area)
    }

    /// The stage: its tab bar, then the view of the tab that is shown.
    fn render_stage(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let content = match TabBar::row(area) {
            Some(row) if !state.stage_tabs().is_empty() => {
                self.tabs.render(frame, row, state);
                let [_, content] =
                    Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
                content
            }
            _ => area,
        };
        match state.shown_tab() {
            Some(StageTab::Project { .. }) => self.project.render(frame, content, state),
            Some(StageTab::Changes { .. }) => self.changes.render(frame, content, state),
            Some(StageTab::Page { .. }) => self.page.render(frame, content, state),
            // Nothing open: the stage says what would put something here.
            None => empty_stage(frame, content, state),
        }
    }

    /// Pasted text (bracketed paste, and what a terminal hands a file drop over as):
    /// paths attach in any mode; other text is typed in while writing.
    pub fn handle_paste(&mut self, text: &str, state: &AppState) -> Vec<Action> {
        self.chat.composer.paste(text, state.mode() == Mode::Insert)
    }
}

/// The empty stage: a pane with nothing in it, and the keys that fill it.
fn empty_stage(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = crate::components::widgets::pane_block(
        Line::from(Span::styled(" stage ", theme::muted())),
        state.ui.focus == Pane::Stage,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "nothing open \u{b7} \u{b7}p a page \u{b7} gd this project's changes \u{b7} the agent's\n  pages and anything it puts on the stage appear here",
            theme::faint(),
        ))),
        inner,
    );
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
            // These modes belong to a view, whatever else is open: you got to them from it.
            Mode::Hint => self.page.handle_key(key, state),
            Mode::Comment => self.changes.handle_key(key, state),
            Mode::Normal => match state.ui.focus {
                Pane::Chat => self.chat.handle_key(key, state),
                // The tab on the screen owns the stage's keys.
                Pane::Stage => match state.shown_tab() {
                    Some(StageTab::Project { .. }) => self.project.handle_key(key, state),
                    Some(StageTab::Changes { .. }) => self.changes.handle_key(key, state),
                    Some(StageTab::Page { .. }) | None => self.page.handle_key(key, state),
                },
            },
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        frame.render_widget(Block::new().style(Style::new().bg(theme::BG)), area);
        let [panes, status] = Self::split(area);
        for (pane, pane_area) in pane_areas(panes, state) {
            match pane {
                Pane::Chat => self.chat.render(frame, pane_area, state),
                Pane::Stage => self.render_stage(frame, pane_area, state),
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
            // The stage gets the wider half: it is where pages and diffs are read.
            let [chat, _, stage] = Layout::horizontal([
                Constraint::Percentage(45),
                Constraint::Length(1),
                Constraint::Percentage(55),
            ])
            .areas(panes);
            vec![(Pane::Chat, chat), (Pane::Stage, stage)]
        }
    }
}
