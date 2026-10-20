use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthChar;

use crate::action::Action;
use crate::components::Component;
use crate::store::AppState;
use crate::store::types::{Mode, UploadState};
use crate::theme;

/// The message input. The text being typed is local state until it is sent.
pub struct Composer {
    text: String,
    /// Byte offset of the cursor in `text`.
    cursor: usize,
}

impl Composer {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }

    /// Height including borders: 1–6 text lines, plus one for attached files.
    pub fn height(&self, width: u16, state: &AppState) -> u16 {
        let lines = wrap_positions(&self.text, width.saturating_sub(4) as usize)
            .len()
            .clamp(1, 6) as u16;
        let files = u16::from(!state.attachments.files.is_empty());
        lines.max(2) + 2 + files
    }

    /// Pasted text (bracketed paste): paths of existing files become attachments, anything
    /// else is typed in.
    pub fn paste(&mut self, text: &str) -> Vec<Action> {
        if let Some(paths) = pasted_paths(text) {
            return paths.into_iter().map(Action::AttachPath).collect();
        }
        for c in text.replace("\r\n", "\n").replace('\r', "\n").chars() {
            self.insert(c);
        }
        vec![]
    }

    pub fn has_draft(&self) -> bool {
        !self.text.trim().is_empty()
    }

    /// Takes the draft for sending (Normal-mode `Enter`).
    pub fn take_draft(&mut self) -> Option<String> {
        let text = self.text.trim().to_string();
        self.text.clear();
        self.cursor = 0;
        (!text.is_empty()).then_some(text)
    }

    fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    fn prev_boundary(&self) -> usize {
        self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next_boundary(&self) -> usize {
        self.text[self.cursor..]
            .chars()
            .next()
            .map_or(self.cursor, |c| self.cursor + c.len_utf8())
    }
}

impl Component for Composer {
    fn handle_key(&mut self, key: KeyEvent, _state: &AppState) -> Vec<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => return vec![Action::SetMode(Mode::Normal)],
            // Enter is a newline while typing; sending is `Esc` then `Enter` (Normal mode).
            (KeyCode::Enter, _) | (KeyCode::Char('j'), KeyModifiers::CONTROL) => self.insert('\n'),
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => return vec![Action::CancelTurn],
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.text.drain(..self.cursor);
                self.cursor = 0;
            }
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                let before = self.text[..self.cursor].trim_end();
                let start = before.rfind(char::is_whitespace).map_or(0, |i| i + 1);
                self.text.drain(start..self.cursor);
                self.cursor = start;
            }
            (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => self.insert(c),
            // Nothing typed: backspace takes back the last attached file.
            (KeyCode::Backspace, _) if self.text.is_empty() => {
                return vec![Action::DropAttachment];
            }
            (KeyCode::Backspace, _) if self.cursor > 0 => {
                let prev = self.prev_boundary();
                self.text.drain(prev..self.cursor);
                self.cursor = prev;
            }
            (KeyCode::Delete, _) if self.cursor < self.text.len() => {
                let next = self.next_boundary();
                self.text.drain(self.cursor..next);
            }
            (KeyCode::Left, _) => self.cursor = self.prev_boundary(),
            (KeyCode::Right, _) => self.cursor = self.next_boundary(),
            (KeyCode::Home, _) | (KeyCode::Char('a'), KeyModifiers::CONTROL) => self.cursor = 0,
            (KeyCode::End, _) | (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                self.cursor = self.text.len()
            }
            _ => {}
        }
        vec![]
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let active = state.mode() == Mode::Insert;
        let hint = match (active, self.has_draft()) {
            (true, _) => " esc · enter to send ",
            (false, true) => " enter to send ",
            (false, false) => "",
        };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::new().fg(if active { theme::OK } else { theme::BORDER }))
            .title_bottom(Line::from(Span::styled(hint, theme::faint())).right_aligned());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let mut inner = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };
        if !state.attachments.files.is_empty() {
            let chips = Rect { height: 1, ..inner };
            frame.render_widget(Paragraph::new(file_chips(state)), chips);
            inner.y += 1;
            inner.height = inner.height.saturating_sub(1);
        }

        if self.text.is_empty() && !active {
            let hint = if state.session.is_busy() {
                "i to write (queued after this turn) · ctrl-c cancels"
            } else {
                "i to write"
            };
            frame.render_widget(Paragraph::new(Span::styled(hint, theme::faint())), inner);
            return;
        }
        let rows = wrap_positions(&self.text, inner.width as usize);
        let (cursor_row, cursor_col) = cursor_position(&rows, &self.text, self.cursor);
        let first = cursor_row.saturating_sub(inner.height as usize - 1);
        let lines: Vec<Line> = rows
            .iter()
            .skip(first)
            .take(inner.height as usize)
            .map(|&(start, end)| {
                Line::from(Span::styled(
                    self.text[start..end].trim_end_matches('\n').to_string(),
                    theme::text(),
                ))
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
        if active {
            frame.set_cursor_position(Position::new(
                inner.x + cursor_col as u16,
                inner.y + (cursor_row - first) as u16,
            ));
        }
    }
}

/// The attached files on one line: `▪ photo.jpg ✓  ▪ notes.md …`.
fn file_chips(state: &AppState) -> Line<'static> {
    let mut spans = Vec::new();
    for file in &state.attachments.files {
        let (mark, style) = match &file.state {
            UploadState::Uploading => (" …", theme::faint()),
            UploadState::Ready(_) => (" ✓", Style::new().fg(theme::OK)),
            UploadState::Failed(_) => (" ✗", Style::new().fg(theme::ERR)),
        };
        spans.push(Span::styled("▪ ", Style::new().fg(theme::ACCENT)));
        spans.push(Span::styled(file.name.clone(), theme::text()));
        spans.push(Span::styled(mark, style));
        spans.push(Span::raw("  "));
    }
    Line::from(spans)
}

/// Pasted text that is only paths of existing files (one per line; quoted, `file://` or
/// `~/` and shell-escaped spaces as terminals and file managers paste them), as paths.
pub fn pasted_paths(text: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() || lines.len() > 20 {
        return None;
    }
    lines
        .into_iter()
        .map(|line| {
            let line = line
                .strip_prefix(['\'', '"'])
                .and_then(|l| l.strip_suffix(['\'', '"']))
                .unwrap_or(line);
            let path = match line.strip_prefix("file://") {
                Some(rest) => percent_decode(rest),
                None => line.replace("\\ ", " "),
            };
            let path = match path.strip_prefix("~/") {
                Some(rest) => format!("{}/{rest}", std::env::var("HOME").ok()?),
                None => path,
            };
            let candidate = std::path::Path::new(&path);
            (candidate.is_absolute() && candidate.is_file()).then_some(path)
        })
        .collect()
}

/// `%20` and friends in a `file://` URL.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Byte ranges of each displayed row (hard newlines and wrapping at `width`).
fn wrap_positions(text: &str, width: usize) -> Vec<(usize, usize)> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            rows.push((start, i + 1));
            start = i + 1;
            used = 0;
            continue;
        }
        let w = c.width().unwrap_or(0);
        if used + w > width {
            rows.push((start, i));
            start = i;
            used = 0;
        }
        used += w;
    }
    rows.push((start, text.len()));
    rows
}

fn cursor_position(rows: &[(usize, usize)], text: &str, cursor: usize) -> (usize, usize) {
    for (row, &(start, end)) in rows.iter().enumerate() {
        let last = row == rows.len() - 1;
        if cursor >= start && (cursor < end || last) {
            let col = text[start..cursor].chars().filter_map(|c| c.width()).sum();
            return (row, col);
        }
    }
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_paths_of_existing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("my notes.md");
        std::fs::write(&file, "x").unwrap();
        let plain = file.display().to_string();
        assert_eq!(pasted_paths(&plain), Some(vec![plain.clone()]));
        assert_eq!(
            pasted_paths(&format!("'{plain}'\n")),
            Some(vec![plain.clone()])
        );
        assert_eq!(
            pasted_paths(&plain.replace(' ', "\\ ")),
            Some(vec![plain.clone()])
        );
        let url = format!("file://{}", plain.replace(' ', "%20"));
        assert_eq!(pasted_paths(&url), Some(vec![plain.clone()]));
        // Text, a folder, a missing file, or a mix: typed in as text.
        assert_eq!(pasted_paths("hello world"), None);
        assert_eq!(pasted_paths(&tmp.path().display().to_string()), None);
        assert_eq!(pasted_paths(&format!("{plain}\nand more")), None);
        assert_eq!(pasted_paths("relative/path.md"), None);
    }
}
