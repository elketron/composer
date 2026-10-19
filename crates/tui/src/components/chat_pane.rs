use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::action::Action;
use crate::components::Component;
use crate::components::chat_lines::chat_lines;
use crate::components::composer::Composer;
use crate::components::widgets::pane_block;
use crate::store::AppState;
use crate::store::types::{Mode, Pane};
use crate::theme;

/// The chat: message history (scrollable, sticks to the bottom) and the composer.
pub struct ChatPane {
    pub composer: Composer,
    /// Lines scrolled up from the bottom; 0 follows new messages.
    scroll: usize,
    /// Visible height at last render, for page-wise scrolling.
    height: Cell<usize>,
    pending_g: bool,
}

impl ChatPane {
    pub fn new() -> Self {
        Self {
            composer: Composer::new(),
            scroll: 0,
            height: Cell::new(20),
            pending_g: false,
        }
    }

    fn title(state: &AppState) -> Line<'static> {
        let sep = || Span::styled(" · ", theme::faint());
        let mut spans = vec![Span::raw(" ")];
        let Some(info) = state.session.current_info() else {
            spans.push(Span::styled("no session ", theme::muted()));
            return Line::from(spans);
        };
        if let Some(parent) = info
            .parent
            .as_ref()
            .and_then(|p| state.session.sessions.iter().find(|s| &s.id == p))
        {
            spans.push(Span::styled(
                parent.agent.clone(),
                Style::new().fg(theme::ACCENT),
            ));
            spans.push(Span::styled(" › ", theme::faint()));
        }
        spans.push(Span::styled(
            info.agent.clone(),
            Style::new().fg(theme::ACCENT),
        ));
        if let Some(project) = &info.project {
            spans.push(sep());
            let name = project.rsplit('/').next().unwrap_or(project);
            spans.push(Span::styled(name.to_string(), theme::text()));
        }
        spans.push(sep());
        spans.push(Span::styled(format!("#{}", info.id), theme::muted()));
        let state_label = match info.state {
            composer_protocol::TurnState::Idle => None,
            composer_protocol::TurnState::Waiting => Some(("waiting for slot", theme::WARN)),
            composer_protocol::TurnState::Running => Some(("running", theme::WARN)),
            composer_protocol::TurnState::AwaitingApproval => Some(("needs approval", theme::WARN)),
        };
        if let Some((label, color)) = state_label {
            spans.push(sep());
            spans.push(Span::styled(label, Style::new().fg(color)));
        }
        let members = state.session.orchestration();
        if let Some(workers) = members.get(1..).filter(|w| !w.is_empty()) {
            let busy = workers
                .iter()
                .filter(|w| {
                    matches!(
                        state.session.status(w),
                        "running" | "waiting" | "approval" | "starting"
                    )
                })
                .count();
            spans.push(sep());
            let color = if busy > 0 { theme::WARN } else { theme::FAINT };
            spans.push(Span::styled(
                format!("{busy}/{} workers busy", workers.len()),
                Style::new().fg(color),
            ));
            spans.push(Span::styled(" (space o)", theme::faint()));
        }
        spans.push(Span::raw(" "));
        Line::from(spans)
    }
}

impl Component for ChatPane {
    fn handle_key(&mut self, key: KeyEvent, state: &AppState) -> Vec<Action> {
        if state.mode() == Mode::Insert {
            return self.composer.handle_key(key, state);
        }
        let page = self.height.get().max(2) / 2;
        let g = std::mem::take(&mut self.pending_g);
        match (key.code, key.modifiers) {
            (KeyCode::Char('k') | KeyCode::Up, _) => self.scroll += 1,
            (KeyCode::Char('j') | KeyCode::Down, _) => self.scroll = self.scroll.saturating_sub(1),
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => self.scroll += page,
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                self.scroll = self.scroll.saturating_sub(page)
            }
            (KeyCode::Char('G'), _) => self.scroll = 0,
            (KeyCode::Char('g'), _) if g => self.scroll = usize::MAX / 2,
            (KeyCode::Char('g'), _) => self.pending_g = true,
            (KeyCode::Enter, _) => return vec![Action::SetMode(Mode::Insert)],
            _ => {}
        }
        vec![]
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let focused = state.ui.focus == Pane::Chat;
        let block = pane_block(Self::title(state), focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let inner = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };

        let composer_height = self.composer.height(inner.width);
        let [messages, input] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(composer_height)])
                .areas(inner);

        let lines = chat_lines(state, messages.width);
        let visible = messages.height as usize;
        self.height.set(visible);
        let max_scroll = lines.len().saturating_sub(visible);
        let scroll = self.scroll.min(max_scroll);
        let start = lines.len().saturating_sub(visible + scroll);
        let shown: Vec<Line> = lines.into_iter().skip(start).take(visible).collect();
        frame.render_widget(Paragraph::new(shown), messages);
        if scroll > 0 {
            let marker = format!(" ↓ {scroll} more ");
            let x = messages.right().saturating_sub(marker.len() as u16 + 1);
            frame.render_widget(
                Paragraph::new(Span::styled(marker, theme::faint())),
                Rect {
                    x,
                    y: messages.bottom() - 1,
                    width: 14,
                    height: 1,
                },
            );
        }
        self.composer.render(frame, input, state);
    }
}
