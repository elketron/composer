use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::action::Action;
use crate::commands::parse_command;
use crate::completion::{Cycle, complete};
use crate::components::Component;
use crate::components::widgets::{fit, pad};
use crate::store::AppState;
use crate::store::types::{FlashLevel, Mode};
use crate::theme;

/// Rows of the completion menu.
const MENU_ROWS: usize = 10;

/// The `:` line, drawn over the status bar in Command mode. Tab completes as vim's command line
/// does: the first match goes in and a menu lists the rest; Tab / ctrl-n and Shift-Tab / ctrl-p
/// cycle (past the end, back to what you typed); any other key keeps the choice.
pub struct CommandLine {
    input: String,
    cycle: Option<Cycle>,
}

impl CommandLine {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cycle: None,
        }
    }

    /// Next (`forward`) or previous candidate; starts cycling on the first press.
    fn step(&mut self, forward: bool, state: &AppState) -> Vec<Action> {
        let cycle = match self.cycle.take() {
            Some(cycle) => cycle,
            None => {
                let completions = complete(&self.input, state);
                match completions.items.len() {
                    0 => return vec![],
                    // One match: take it, no menu.
                    1 => {
                        self.input.truncate(completions.start);
                        self.input.push_str(&completions.items[0].text);
                        return vec![];
                    }
                    _ => Cycle {
                        original: self.input.clone(),
                        start: completions.start,
                        items: completions.items,
                        selected: None,
                    },
                }
            }
        };
        let count = cycle.items.len();
        let selected = match (cycle.selected, forward) {
            (None, true) => Some(0),
            (None, false) => Some(count - 1),
            (Some(i), true) if i + 1 < count => Some(i + 1),
            (Some(i), false) if i > 0 => Some(i - 1),
            _ => None,
        };
        self.input = match selected {
            Some(i) => format!("{}{}", &cycle.original[..cycle.start], cycle.items[i].text),
            None => cycle.original.clone(),
        };
        self.cycle = Some(Cycle { selected, ..cycle });
        vec![]
    }

    fn render_menu(&self, frame: &mut Frame, area: Rect) {
        let Some(cycle) = &self.cycle else { return };
        let screen = frame.area();
        let rows = cycle.items.len().min(MENU_ROWS).min(area.y as usize);
        if rows == 0 {
            return;
        }
        let text_width = cycle
            .items
            .iter()
            .map(|c| c.text.width())
            .max()
            .unwrap_or(0)
            .min(48);
        let detail_width = cycle
            .items
            .iter()
            .map(|c| c.detail.width())
            .max()
            .unwrap_or(0)
            .min(56);
        let width = (1
            + text_width
            + if detail_width > 0 {
                2 + detail_width
            } else {
                0
            }
            + 1)
        .min(screen.width as usize) as u16;
        // Under the word being completed, kept on screen.
        let column = area.x + 1 + cycle.original[..cycle.start].width() as u16;
        let x = column.min(screen.width.saturating_sub(width));
        let rect = Rect::new(x, area.y - rows as u16, width, rows as u16);

        let selected = cycle.selected.unwrap_or(0);
        let first = selected.saturating_sub(rows - 1);
        let detail_room = (width as usize).saturating_sub(text_width + 4);
        let lines: Vec<Line> = cycle
            .items
            .iter()
            .enumerate()
            .skip(first)
            .take(rows)
            .map(|(i, item)| {
                let on = cycle.selected == Some(i);
                let bg = if on { theme::RAISED } else { theme::SURFACE };
                let text_style = if on {
                    Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD)
                } else {
                    theme::text()
                };
                let mut spans = vec![
                    Span::styled(if on { "▎" } else { " " }, Style::new().fg(theme::ACCENT)),
                    Span::styled(pad(&fit(&item.text, text_width), text_width), text_style),
                ];
                if detail_room > 0 {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        pad(&fit(&item.detail, detail_room), detail_room),
                        theme::faint(),
                    ));
                }
                spans.push(Span::raw(" "));
                Line::from(spans).style(Style::new().bg(bg))
            })
            .collect();
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Paragraph::new(lines).style(Style::new().bg(theme::SURFACE)),
            rect,
        );
    }
}

impl Component for CommandLine {
    fn handle_key(&mut self, key: KeyEvent, state: &AppState) -> Vec<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Tab => return self.step(true, state),
            KeyCode::BackTab => return self.step(false, state),
            KeyCode::Char('n') if ctrl => return self.step(true, state),
            KeyCode::Char('p') if ctrl => return self.step(false, state),
            KeyCode::Down if self.cycle.is_some() => return self.step(true, state),
            KeyCode::Up if self.cycle.is_some() => return self.step(false, state),
            _ => {}
        }
        // Any other key keeps the line as it is (with the chosen candidate) and goes on from there.
        self.cycle = None;
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
            KeyCode::Char(c) if !ctrl => {
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
        self.render_menu(frame, area);
        frame.set_cursor_position(Position::new(
            area.x + 1 + self.input.width() as u16,
            area.y,
        ));
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::store::types::Startup;

    fn press(line: &mut CommandLine, state: &AppState, keys: &str) -> Vec<Action> {
        let mut actions = Vec::new();
        for key in crate::headless::parse_keys(keys).unwrap() {
            actions.extend(line.handle_key(key, state));
        }
        actions
    }

    fn state() -> AppState {
        AppState::new(Startup::default(), 160_000)
    }

    #[test]
    fn tab_cycles_and_returns_to_what_was_typed() {
        let state = state();
        let mut line = CommandLine::new();
        press(&mut line, &state, "c");
        // compact is the only command starting with c: taken at once, no menu.
        press(&mut line, &state, "<Tab>");
        assert_eq!(line.input, "compact");
        assert!(line.cycle.is_none());

        let mut line = CommandLine::new();
        press(&mut line, &state, "a<Tab>");
        assert_eq!(line.input, "attach");
        press(&mut line, &state, "<Tab>");
        assert_eq!(line.input, "app");
        press(&mut line, &state, "<Tab>");
        assert_eq!(line.input, "a", "past the last: back to what was typed");
        press(&mut line, &state, "<S-Tab>");
        assert_eq!(line.input, "app");
        line.handle_key(
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
            &state,
        );
        assert_eq!(line.input, "attach");
    }

    #[test]
    fn typing_keeps_the_choice_and_enter_runs_it() {
        let state = state();
        let mut line = CommandLine::new();
        press(&mut line, &state, "he<Tab>");
        assert_eq!(line.input, "help");
        let actions = press(&mut line, &state, "<Enter>");
        assert_eq!(actions, [Action::SetMode(Mode::Normal), Action::ToggleHelp]);

        let mut line = CommandLine::new();
        press(&mut line, &state, "a<Tab><Space>");
        assert_eq!(line.input, "attach ");
        assert!(line.cycle.is_none());
    }

    #[test]
    fn nothing_to_complete_changes_nothing() {
        let state = state();
        let mut line = CommandLine::new();
        press(&mut line, &state, "zz<Tab>");
        assert_eq!(line.input, "zz");
        assert!(line.cycle.is_none());
    }
}
