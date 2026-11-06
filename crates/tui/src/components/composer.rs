use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthChar;

use crate::action::Action;
use crate::components::Component;
use crate::row::Row;
use crate::store::AppState;
use crate::store::types::{FlashLevel, Mode, UploadState};
use crate::theme;

/// The message input. The text being typed is local state until it is sent.
pub struct Composer {
    text: String,
    /// Byte offset of the cursor in `text`.
    cursor: usize,
    /// Text width at the last render, so the arrow keys move between displayed lines.
    wrap_width: Cell<usize>,
}

impl Composer {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            wrap_width: Cell::new(usize::MAX),
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

    /// Pasted text (bracketed paste — a terminal also hands a dropped file over this way,
    /// even when nothing is being typed): paths of existing files become attachments, a
    /// path that cannot be read flashes an error and attaches nothing, anything else is
    /// typed in while writing.
    pub fn paste(&mut self, text: &str, writing: bool) -> Vec<Action> {
        match pasted_paths(text) {
            Ok(Some(paths)) => return paths.into_iter().map(Action::AttachPath).collect(),
            Err(why) => return vec![Action::Flash(why, FlashLevel::Warn)],
            Ok(None) => {}
        }
        if !writing {
            return vec![];
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

    /// Move the cursor `delta` displayed lines away, keeping the column where the
    /// line allows it (clamped to its end). Off the first/last line: nothing.
    fn move_cursor_line(&mut self, delta: i32) {
        let rows = wrap_positions(&self.text, self.wrap_width.get());
        let (row, col) = cursor_position(&rows, &self.text, self.cursor);
        let Ok(target) = i32::try_from(row).map(|r| r + delta) else {
            return;
        };
        let Ok(target) = usize::try_from(target) else {
            return;
        };
        if target >= rows.len() {
            return;
        }
        let target_row = rows[target];
        let mut pos = target_row.offset_at_column(&self.text, col);

        // The byte at a wrapped line's end starts the next line, so a cursor parked at a
        // full-width column cannot rest there: stop at the line's last character instead.
        if !target_row.hard_break(&self.text) && target < rows.len() - 1 && pos >= target_row.end()
        {
            pos = target_row.last_char_offset(&self.text);
        }
        self.cursor = pos;
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
            (KeyCode::Up, _) => self.move_cursor_line(-1),
            (KeyCode::Down, _) => self.move_cursor_line(1),
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
        self.wrap_width.set(inner.width as usize);
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
            .map(|row| {
                Line::from(Span::styled(
                    row.content(&self.text).to_string(),
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

/// Pasted text that is only paths of existing readable files (one per line; quoted,
/// `file://` or `~/` and shell-escaped spaces as terminals and file managers paste them,
/// and as a terminal hands a dropped file to the program), as paths. A path of a file
/// that is there but cannot be read is an `Err` (a clear error, nothing attached); text
/// that is not all paths is `Ok(None)`.
pub fn pasted_paths(text: &str) -> Result<Option<Vec<String>>, String> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() || lines.len() > 20 {
        return Ok(None);
    }
    let mut paths = Vec::new();
    for line in lines {
        let path = one_path(line);
        let Some(path) = path else {
            return Ok(None);
        };
        if let Err(e) = std::fs::File::open(&path) {
            return Err(format!("cannot read {path}: {e}"));
        }
        paths.push(path);
    }
    Ok(Some(paths))
}

/// One line as the path of an existing file, or `None`.
fn one_path(line: &str) -> Option<String> {
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

/// A constructed range never runs backwards; the type keeps it that way.
fn row(start: usize, end: usize) -> Row {
    Row::new(start, end).expect("a constructed range never runs backwards")
}

/// Byte ranges of each displayed row (hard newlines and wrapping at `width`).
fn wrap_positions(text: &str, width: usize) -> Vec<Row> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            rows.push(row(start, i + 1));
            start = i + 1;
            used = 0;
            continue;
        }
        let w = c.width().unwrap_or(0);
        if used + w > width {
            rows.push(row(start, i));
            start = i;
            used = 0;
        }
        used += w;
    }
    rows.push(row(start, text.len()));
    rows
}

fn cursor_position(rows: &[Row], text: &str, cursor: usize) -> (usize, usize) {
    for (i, row) in rows.iter().enumerate() {
        let last = i == rows.len() - 1;
        if row.holds_cursor(cursor, last) {
            return (i, row.column_of(text, cursor));
        }
    }
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::types::Startup;

    fn press(composer: &mut Composer, state: &AppState, keys: &str) {
        for key in crate::headless::parse_keys(keys).unwrap() {
            composer.handle_key(key, state);
        }
    }

    fn state() -> AppState {
        AppState::new(Startup::default(), 160_000)
    }

    #[test]
    fn arrows_move_the_cursor_between_lines() {
        let state = state();
        let mut composer = Composer::new();
        press(&mut composer, &state, "ab<Enter>cdef<Enter>ghi");
        assert_eq!(composer.cursor, 11, "the cursor starts at the end");
        press(&mut composer, &state, "<Up>");
        assert_eq!(composer.cursor, 6, "up keeps the column on the middle line");
        press(&mut composer, &state, "<Up>");
        assert_eq!(
            composer.cursor, 2,
            "the shorter first line clamps the column"
        );
        press(&mut composer, &state, "<Up>");
        assert_eq!(composer.cursor, 2, "off the first line: still nothing");
        press(&mut composer, &state, "<Down>");
        assert_eq!(composer.cursor, 5, "down keeps the column");
        press(&mut composer, &state, "<Down>");
        assert_eq!(composer.cursor, 10, "back on the last line");
        press(&mut composer, &state, "<Down>");
        assert_eq!(composer.cursor, 10, "off the last line: still nothing");
    }

    #[test]
    fn down_onto_an_empty_last_row_lands_on_it() {
        let state = state();
        let mut composer = Composer::new();
        press(&mut composer, &state, "abcd<Enter>");
        press(&mut composer, &state, "<Up>");
        assert_eq!(
            composer.cursor, 0,
            "up from the empty row keeps its column (0)"
        );
        press(&mut composer, &state, "<Down>");
        assert_eq!(
            composer.cursor, 5,
            "the row below is empty: land at its start"
        );
    }

    #[test]
    fn arrows_move_between_wrapped_lines() {
        let state = state();
        let mut composer = Composer::new();
        composer.wrap_width.set(4);
        press(&mut composer, &state, "abcd<Enter>ef");
        press(&mut composer, &state, "<Up>");
        assert_eq!(
            composer.cursor, 2,
            "up a wrapped line, clamped to its column"
        );
        press(&mut composer, &state, "<Down>");
        assert_eq!(composer.cursor, 7, "and back down");
    }
    #[test]
    fn a_full_wrapped_line_ends_one_character_short() {
        let state = state();
        let mut composer = Composer::new();
        composer.wrap_width.set(4);
        press(&mut composer, &state, "abcdefgh");
        assert_eq!(
            composer.cursor, 8,
            "the cursor sits at the end of the last row"
        );
        press(&mut composer, &state, "<Up>");
        assert_eq!(
            composer.cursor, 3,
            "the row above is full too: land on its last character, not on the row break"
        );
    }

    #[test]
    fn arrows_use_the_width_the_last_render_had() {
        let state = state();
        let mut composer = Composer::new();
        press(&mut composer, &state, "abcdefgh<Enter>xy");
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(8, 6)).unwrap();
        terminal
            .draw(|frame| composer.render(frame, frame.area(), &state))
            .unwrap();
        // Rows of 4: "abcd", "efgh", "xy"; the cursor starts at the end (byte 11).
        assert_eq!(composer.cursor, 11);
        press(&mut composer, &state, "<Up>");
        assert_eq!(composer.cursor, 6, "one row up, at the same column");
        press(&mut composer, &state, "<Up>");
        assert_eq!(composer.cursor, 2, "and up again");
    }

    #[test]
    fn pasted_paths_of_existing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("my notes.md");
        std::fs::write(&file, "x").unwrap();
        let plain = file.display().to_string();
        assert_eq!(pasted_paths(&plain), Ok(Some(vec![plain.clone()])));
        assert_eq!(
            pasted_paths(&format!("'{plain}'\n")),
            Ok(Some(vec![plain.clone()]))
        );
        assert_eq!(
            pasted_paths(&plain.replace(' ', "\\ ")),
            Ok(Some(vec![plain.clone()]))
        );
        let url = format!("file://{}", plain.replace(' ', "%20"));
        assert_eq!(pasted_paths(&url), Ok(Some(vec![plain.clone()])));
        // A drop of several files arrives as one paste with a path per line.
        let second = tmp.path().join("b.png");
        std::fs::write(&second, "x").unwrap();
        assert_eq!(
            pasted_paths(&format!("{plain}\n{}", second.display())),
            Ok(Some(vec![plain.clone(), second.display().to_string()]))
        );
        // Text, a folder, a missing file, or a mix: typed in as text.
        assert_eq!(pasted_paths("hello world"), Ok(None));
        assert_eq!(pasted_paths(&tmp.path().display().to_string()), Ok(None));
        assert_eq!(pasted_paths(&format!("{plain}\nand more")), Ok(None));
        assert_eq!(pasted_paths("relative/path.md"), Ok(None));
    }

    #[test]
    fn a_dropped_path_attaches_even_while_not_typing() {
        // A terminal hands a file drop over as a bracketed paste, whether or not the
        // composer was writing when it landed.
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("photo.png");
        std::fs::write(&file, "x").unwrap();
        let plain = file.display().to_string();
        let mut composer = Composer::new();
        assert_eq!(
            composer.paste(&format!("file://{plain}"), false),
            vec![Action::AttachPath(plain.clone())]
        );
        assert_eq!(composer.text, "", "the path is not typed into the draft");
        // Plain text while not writing: still ignored.
        assert_eq!(composer.paste("hello", false), vec![]);
        assert_eq!(composer.text, "");
    }

    #[test]
    #[cfg(unix)]
    fn an_unreadable_path_errors_and_attaches_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("secret.png");
        std::fs::write(&file, "x").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let plain = file.display().to_string();
        let error = pasted_paths(&plain).unwrap_err();
        assert!(error.starts_with("cannot read "), "{error}");
        let mut composer = Composer::new();
        let actions = composer.paste(&plain, true);
        match actions.as_slice() {
            [Action::Flash(why, FlashLevel::Warn)] => {
                assert!(why.starts_with("cannot read "), "{why}")
            }
            other => panic!("expected one flash, got {other:?}"),
        }
        assert_eq!(composer.text, "", "the path is not typed in either");
    }
}
