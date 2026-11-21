use std::fmt;
use std::sync::Arc;

use tattoy_wezterm_term as wt;
use wezterm_surface::CursorVisibility;
use wt::color::ColorAttribute;
use wt::{KeyCode, KeyModifiers, Terminal, TerminalSize};

use crate::grid::Scrollback;
use crate::output::{Output, Sink};
use crate::term_config::{Config, Osc52};
use crate::types::{CellView, Paint};

/// A terminal's screen: wezterm-term's model of what a program drew, how far back the view is
/// scrolled, and what is to be sent to the program.
pub struct TermScreen {
    term: Terminal,
    output: Output,
    osc52: Arc<Osc52>,
    size: (usize, usize),
    scroll: Scrollback,
}

impl fmt::Debug for TermScreen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TermScreen")
            .field("size", &self.size)
            .field("scroll", &self.scroll)
            .finish_non_exhaustive()
    }
}

impl TermScreen {
    /// What it writes to the program is kept for [`TermScreen::take_output`].
    pub fn new(cols: usize, rows: usize) -> Self {
        Self::with_sink(cols, rows, None)
    }

    /// What it writes to the program goes to `sink` as it comes (a moment after the key).
    pub fn with_sink(cols: usize, rows: usize, sink: Option<Sink>) -> Self {
        let (cols, rows) = (cols.max(1), rows.max(1));
        let output = Output::new(sink);
        let mut term = Terminal::new(
            terminal_size(cols, rows),
            Arc::new(Config),
            "composer",
            env!("CARGO_PKG_VERSION"),
            Box::new(output.clone()),
        );
        let osc52 = Arc::new(Osc52::default());
        let clip: Arc<dyn wt::Clipboard> = osc52.clone();
        term.set_clipboard(&clip);
        Self {
            term,
            output,
            osc52,
            size: (cols, rows),
            scroll: Scrollback::default(),
        }
    }

    /// Bytes from the program. What it asks (a cursor report) is answered in the output.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.term.advance_bytes(bytes);
    }

    /// Old output (a terminal's scrollback when the view opens): drawn, never answered.
    pub fn replay(&mut self, bytes: &[u8]) {
        self.output.quiet();
        self.term.advance_bytes(bytes);
        self.output.quiet();
    }

    /// A key, encoded as the program asked; the view goes back to the live screen.
    pub fn key(&mut self, code: KeyCode, mods: KeyModifiers) {
        self.scroll = Scrollback::default();
        self.output.unquiet();
        let _ = self.term.key_down(code, mods);
    }

    /// Text as a paste (bracketed when the program asked).
    pub fn paste(&mut self, text: &str) {
        self.scroll = Scrollback::default();
        self.output.unquiet();
        let _ = self.term.send_paste(text);
    }

    /// What was written to the program since the last take (without a sink; it arrives a
    /// moment after the key).
    pub fn take_output(&self) -> Vec<u8> {
        self.output.take()
    }

    /// Text the program put on the clipboard (OSC 52), once.
    pub fn take_clipboard(&self) -> Option<String> {
        self.osc52.0.lock().expect("osc52 lock").take()
    }

    /// Columns and rows.
    pub fn size(&self) -> (usize, usize) {
        self.size
    }

    /// A new size; whether it changed.
    pub fn resize(&mut self, cols: usize, rows: usize) -> bool {
        let size = (cols.max(1), rows.max(1));
        if size == self.size {
            return false;
        }
        self.size = size;
        self.term.resize(terminal_size(size.0, size.1));
        true
    }

    /// Lines of history above the live screen.
    pub fn history(&self) -> usize {
        let screen = self.term.screen();
        screen.scrollback_rows() - screen.physical_rows
    }

    /// Scrolls the view by `lines` (positive: back in time). In a full-screen program (the
    /// alternate screen, no history) the lines go to it as arrow keys instead, as terminals do.
    pub fn scroll_by(&mut self, lines: i64) {
        if self.term.is_alt_screen_active() {
            let key = if lines > 0 {
                KeyCode::UpArrow
            } else {
                KeyCode::DownArrow
            };
            for _ in 0..lines.unsigned_abs() {
                let _ = self.term.key_down(key, KeyModifiers::NONE);
            }
            return;
        }
        self.scroll = self.scroll.scrolled(lines, self.history());
    }

    /// How far back the view is, in lines (0: the live screen).
    pub fn scrolled(&self) -> usize {
        self.scroll.lines()
    }

    /// The rows the view shows, top first: each its cells.
    pub fn rows(&self) -> Vec<Vec<CellView>> {
        let top = self.scroll.top_row(self.history());
        let screen = self.term.screen();
        let end = (top + screen.physical_rows).min(screen.scrollback_rows());
        screen
            .lines_in_phys_range(top..end)
            .iter()
            .map(|line| line.visible_cells().map(|cell| cell_view(&cell)).collect())
            .collect()
    }

    /// The rows as plain text (trailing blanks trimmed).
    pub fn text(&self) -> Vec<String> {
        self.rows()
            .iter()
            .map(|row| {
                let mut text = String::new();
                for cell in row {
                    while text.chars().count() < cell.col {
                        text.push(' ');
                    }
                    text.push_str(if cell.text.is_empty() {
                        " "
                    } else {
                        &cell.text
                    });
                }
                text.trim_end().to_string()
            })
            .collect()
    }

    /// Where the cursor is on the view (column, row), when it shows: on the live screen only.
    pub fn cursor(&self) -> Option<(usize, usize)> {
        let cursor = self.term.cursor_pos();
        let shown = self.scroll.lines() == 0
            && cursor.visibility == CursorVisibility::Visible
            && cursor.y >= 0;
        shown.then_some((cursor.x, cursor.y as usize))
    }

    /// The program asked for the mouse.
    pub fn mouse_grabbed(&self) -> bool {
        self.term.is_mouse_grabbed()
    }
}

fn terminal_size(cols: usize, rows: usize) -> TerminalSize {
    TerminalSize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
        dpi: 96,
    }
}

fn cell_view(cell: &wt::CellRef) -> CellView {
    let attrs = cell.attrs();
    CellView {
        col: cell.cell_index(),
        text: cell.str().to_string(),
        width: cell.width().max(1),
        fg: paint(attrs.foreground()),
        bg: paint(attrs.background()),
        bold: attrs.intensity() == wt::Intensity::Bold,
        dim: attrs.intensity() == wt::Intensity::Half,
        italic: attrs.italic(),
        underline: attrs.underline() != wt::Underline::None,
        strikethrough: attrs.strikethrough(),
        reverse: attrs.reverse(),
        invisible: attrs.invisible(),
    }
}

fn paint(color: ColorAttribute) -> Paint {
    match color {
        ColorAttribute::Default => Paint::Default,
        ColorAttribute::PaletteIndex(index) => Paint::Index(index),
        ColorAttribute::TrueColorWithPaletteFallback(rgba, _)
        | ColorAttribute::TrueColorWithDefaultFallback(rgba) => {
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            Paint::Rgb(byte(rgba.0), byte(rgba.1), byte(rgba.2))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the screen wrote, once it is `len` bytes (it arrives from another thread).
    fn output(screen: &TermScreen, len: usize) -> Vec<u8> {
        let mut seen = Vec::new();
        for _ in 0..200 {
            seen.extend(screen.take_output());
            if seen.len() >= len {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        seen
    }

    #[test]
    fn draws_styles_and_answers() {
        let mut screen = TermScreen::new(20, 4);
        screen.feed(b"hello \x1b[1;31mred\x1b[0m\r\n\x1b[38;2;10;20;30mrgb\x1b[0m");
        let text = screen.text();
        assert_eq!(text[0], "hello red");
        assert_eq!(text[1], "rgb");
        let rows = screen.rows();
        let r = rows[0].iter().find(|c| c.text == "r").unwrap();
        assert!(r.bold);
        assert_eq!(r.fg, Paint::Index(1));
        assert_eq!(rows[1][0].fg, Paint::Rgb(10, 20, 30));
        assert_eq!(screen.cursor(), Some((3, 1)));
        // A cursor position query is answered; a replayed one is not.
        screen.replay(b"\x1b[6n");
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(screen.take_output().is_empty());
        screen.feed(b"\x1b[6n");
        assert_eq!(output(&screen, 6), b"\x1b[2;4R");
    }

    #[test]
    fn keys_scrollback_and_resize() {
        let mut screen = TermScreen::new(10, 3);
        screen.key(KeyCode::Char('a'), KeyModifiers::NONE);
        screen.key(KeyCode::Char('c'), KeyModifiers::CTRL);
        screen.key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(output(&screen, 3), b"a\x03\r");
        for i in 0..10 {
            screen.feed(format!("line {i}\r\n").as_bytes());
        }
        assert_eq!(screen.text().last().unwrap(), "");
        screen.scroll_by(3);
        assert_eq!(screen.scrolled(), 3);
        assert_eq!(screen.cursor(), None, "no cursor while scrolled back");
        assert_eq!(screen.text()[0], "line 5");
        screen.key(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(screen.scrolled(), 0, "a key goes back to the live screen");
        assert!(screen.resize(30, 5));
        assert!(!screen.resize(30, 5));
        assert_eq!(screen.size(), (30, 5));
    }
}
