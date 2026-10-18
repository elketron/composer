use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::action::Action;
use crate::commands::parse_command;
use crate::components::Component;
use crate::store::AppState;
use crate::store::types::{FlashLevel, Mode};
use crate::theme;

/// The `:` line, drawn over the status bar in Command mode.
pub struct CommandLine {
    input: String,
}

impl CommandLine {
    pub fn new() -> Self {
        Self {
            input: String::new(),
        }
    }
}

impl Component for CommandLine {
    fn handle_key(&mut self, key: KeyEvent, _state: &AppState) -> Vec<Action> {
        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                vec![Action::SetMode(Mode::Normal)]
            }
            KeyCode::Enter => {
                let line = std::mem::take(&mut self.input);
                let mut actions = vec![Action::SetMode(Mode::Normal)];
                match parse_command(&line) {
                    Ok(action) => actions.push(action),
                    Err(error) if !error.is_empty() => {
                        actions.push(Action::Flash(error, FlashLevel::Error))
                    }
                    Err(_) => {}
                }
                actions
            }
            KeyCode::Backspace if self.input.is_empty() => vec![Action::SetMode(Mode::Normal)],
            KeyCode::Backspace => {
                self.input.pop();
                vec![]
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                vec![]
            }
            _ => vec![],
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, _state: &AppState) {
        let line = Line::from(vec![
            Span::styled(":", Style::new().fg(theme::INFO)),
            Span::styled(self.input.clone(), theme::text()),
        ]);
        frame.render_widget(Paragraph::new(line).style(Style::new().bg(theme::BG)), area);
        frame.set_cursor_position(Position::new(
            area.x + 1 + self.input.chars().count() as u16,
            area.y,
        ));
    }
}
