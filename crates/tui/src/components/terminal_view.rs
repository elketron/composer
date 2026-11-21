//! A server terminal on the stage: its screen (`composer-term`) painted cell by cell.

use composer_term::{CellView, Paint};
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::action::Action;
use crate::components::Component;
use crate::components::widgets::pane_block;
use crate::store::AppState;
use crate::store::types::Pane;
use crate::theme;

/// Draws the terminal shown on the stage. Its keys reach it through the app (they bypass the
/// keymap while it has the focus), so it keeps no state of its own.
pub struct TerminalView;

impl TerminalView {
    /// Where the terminal's cells go inside the stage's `content` area: inside the pane's
    /// border.
    pub fn grid(content: Rect) -> Rect {
        pane_block(Line::default(), false).inner(content)
    }
}

impl Component for TerminalView {
    fn handle_key(&mut self, key: KeyEvent, _state: &AppState) -> Vec<Action> {
        vec![Action::TerminalKey(key)]
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let Some(id) = state.shown_terminal() else {
            return;
        };
        let focused = state.ui.focus == Pane::Stage;
        let screen = state.terminals.screen(&id);
        let mut title = vec![Span::styled(
            match state.terminals.process(&id) {
                Some(process) => format!(" $ {} ", process.command),
                None => format!(" term \u{b7} {id} "),
            },
            if focused {
                theme::bold()
            } else {
                theme::muted()
            },
        )];
        if let Some(back) = screen.map(|s| s.scrolled()).filter(|n| *n > 0) {
            title.push(Span::styled(format!("\u{2191}{back} "), theme::faint()));
        }
        if state.terminals.ended.contains(&id) {
            title.push(Span::styled(
                "ended \u{b7} Enter: a new shell ",
                theme::faint(),
            ));
        } else if state.terminal_focused() {
            title.push(Span::styled(
                "ctrl-space: back to the chat ",
                theme::faint(),
            ));
        }
        let block = pane_block(Line::from(title), focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let Some(screen) = screen else {
            let note = if state.terminals.ended.contains(&id) {
                "the terminal ended"
            } else {
                "reading the terminal\u{2026}"
            };
            frame.render_widget(Paragraph::new(Span::styled(note, theme::faint())), inner);
            return;
        };
        let cursor = state.terminal_focused().then(|| screen.cursor()).flatten();
        let buffer = frame.buffer_mut();
        for (y, row) in screen.rows().iter().enumerate() {
            if y as u16 >= inner.height {
                break;
            }
            for cell in row {
                if cell.col as u16 >= inner.width {
                    break;
                }
                let x = inner.x + cell.col as u16;
                let y = inner.y + y as u16;
                let mut style = cell_style(cell);
                if cursor == Some((cell.col, y as usize - inner.y as usize)) {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                let text = if cell.text.is_empty() {
                    " "
                } else {
                    &cell.text
                };
                let room = (inner.right() - x) as usize;
                buffer.set_stringn(x, y, text, room.min(cell.width), style);
            }
        }
        // The cursor past the last cell of its row (an empty prompt line).
        if let Some((col, row)) = cursor
            && (col as u16) < inner.width
            && (row as u16) < inner.height
        {
            let cell = &mut buffer[(inner.x + col as u16, inner.y + row as u16)];
            if cell.symbol() == " " || cell.symbol().is_empty() {
                cell.set_style(Style::new().add_modifier(Modifier::REVERSED));
            }
        }
    }
}

fn color(paint: Paint) -> Option<Color> {
    match paint {
        Paint::Default => None,
        Paint::Index(index) => Some(Color::Indexed(index)),
        Paint::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

fn cell_style(cell: &CellView) -> Style {
    let mut style = Style::new().fg(color(cell.fg).unwrap_or(theme::TEXT));
    if let Some(bg) = color(cell.bg) {
        style = style.bg(bg);
    }
    for (on, modifier) in [
        (cell.bold, Modifier::BOLD),
        (cell.dim, Modifier::DIM),
        (cell.italic, Modifier::ITALIC),
        (cell.underline, Modifier::UNDERLINED),
        (cell.strikethrough, Modifier::CROSSED_OUT),
        (cell.reverse, Modifier::REVERSED),
        (cell.invisible, Modifier::HIDDEN),
    ] {
        if on {
            style = style.add_modifier(modifier);
        }
    }
    style
}
