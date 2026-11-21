use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::action::Action;
use crate::commands::session_label;
use crate::components::Component;
use crate::components::widgets::{centered, dim};
use crate::store::AppState;
use crate::theme;

/// Asks before `:delete` deletes a session. Modal: `y` deletes, any other key keeps it.
pub struct ConfirmPopup;

impl Component for ConfirmPopup {
    fn handle_key(&mut self, key: KeyEvent, _state: &AppState) -> Vec<Action> {
        vec![Action::ConfirmDelete(key.code == KeyCode::Char('y'))]
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let Some(id) = &state.ui.confirm_delete else {
            return;
        };
        dim(frame.buffer_mut(), area);
        let rect = centered(area, 72, 7);
        frame.render_widget(Clear, rect);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::new().fg(theme::ERR))
            .title(Span::styled(
                " delete session? ",
                Style::new().fg(theme::ERR),
            ))
            .style(Style::new().bg(theme::SURFACE));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let inner = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };
        let info = state.session.sessions.iter().find(|s| &s.id == id);
        let lines = vec![
            Line::from(vec![
                Span::styled(format!("#{id} "), theme::bold()),
                Span::styled(info.map(session_label).unwrap_or_default(), theme::muted()),
                Span::styled(
                    format!(
                        "  {}",
                        info.and_then(|s| s.preview.as_deref()).unwrap_or("")
                    ),
                    theme::faint(),
                ),
            ]),
            Line::raw(""),
            Line::from(Span::styled(
                "its chat, its turn, its terminals and its folder go, for every client",
                theme::faint(),
            )),
            Line::from(vec![
                Span::styled("y", Style::new().fg(theme::ERR)),
                Span::styled(" delete   ", theme::faint()),
                Span::styled("any other key", theme::muted()),
                Span::styled(" keep it", theme::faint()),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}
