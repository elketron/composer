//! The message being written, with the TUI's modal keys: in NORMAL, `i` / `a`
//! write at / after the cursor, `o` on a new line at the end, Enter sends, `h` `l` `0` `$` move,
//! `x` deletes; in INSERT, typing writes, Enter is a new line, Escape goes back. `ctrl+enter`
//! sends from either.

/// What a key did to the draft.
#[derive(Debug, Clone, PartialEq)]
pub enum DraftKey {
    /// Not a draft key (the pane passes it on).
    Unhandled,
    Changed,
    /// Send this text (the draft is empty again).
    Send(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
}

/// The text, the cursor (a byte offset on a char boundary) and the mode.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Draft {
    pub text: String,
    pub cursor: usize,
    pub mode: Mode,
}

impl Draft {
    /// A key: its name (`enter`, `backspace`, `a`), the text it types, and whether control or
    /// the platform key is held.
    pub fn key(&mut self, key: &str, typed: Option<&str>, control: bool) -> DraftKey {
        if control {
            return match key {
                "enter" => self.send(),
                "u" if self.mode == Mode::Insert => {
                    // Deletes back to the start of the line.
                    let start = self.line_start();
                    self.text.drain(start..self.cursor);
                    self.cursor = start;
                    DraftKey::Changed
                }
                _ => DraftKey::Unhandled,
            };
        }
        match self.mode {
            Mode::Insert => self.insert_key(key, typed),
            Mode::Normal => self.normal_key(key, typed),
        }
    }

    fn insert_key(&mut self, key: &str, typed: Option<&str>) -> DraftKey {
        match key {
            "escape" => {
                self.mode = Mode::Normal;
                // As in vim, the cursor steps back onto the last character written.
                if self.cursor > self.line_start() {
                    self.left();
                }
            }
            "enter" => self.insert("\n"),
            "backspace" => self.backspace(),
            "delete" => self.delete(),
            "left" => self.left(),
            "right" => self.right(),
            "home" => self.cursor = self.line_start(),
            "end" => self.cursor = self.line_end(),
            "tab" => self.insert("    "),
            _ => match typed.filter(|t| !t.is_empty() && !t.chars().any(char::is_control)) {
                Some(text) => self.insert(text),
                None => return DraftKey::Unhandled,
            },
        }
        DraftKey::Changed
    }

    fn normal_key(&mut self, key: &str, typed: Option<&str>) -> DraftKey {
        match typed.unwrap_or(key) {
            "i" => self.mode = Mode::Insert,
            "a" => {
                self.right();
                self.mode = Mode::Insert;
            }
            "A" => {
                self.cursor = self.line_end();
                self.mode = Mode::Insert;
            }
            "I" => {
                self.cursor = self.line_start();
                self.mode = Mode::Insert;
            }
            "o" => {
                self.cursor = self.text.len();
                if !self.text.is_empty() {
                    self.insert("\n");
                }
                self.mode = Mode::Insert;
            }
            "h" => self.left(),
            "l" => {
                // Normal mode stays on a character, not after the last one.
                if self.next_boundary() < self.line_end() {
                    self.right();
                }
            }
            "0" => self.cursor = self.line_start(),
            "$" => self.cursor = self.last_char_of_line(),
            "x" => {
                if self.cursor < self.line_end() {
                    self.delete();
                    self.cursor = self.cursor.min(self.last_char_of_line());
                }
            }
            _ => match key {
                "enter" => return self.send(),
                "left" => self.left(),
                "right" => self.right(),
                _ => return DraftKey::Unhandled,
            },
        }
        DraftKey::Changed
    }

    /// Text put in at the cursor (a paste, a dropped file's path): writing goes on after it.
    pub fn insert(&mut self, text: &str) {
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    /// The text to send, trimmed; the draft empties and goes back to NORMAL.
    pub fn send(&mut self) -> DraftKey {
        let text = self.text.trim().to_string();
        if text.is_empty() {
            return DraftKey::Unhandled;
        }
        *self = Draft::default();
        DraftKey::Send(text)
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            let start = self.prev_boundary();
            self.text.drain(start..self.cursor);
            self.cursor = start;
        }
    }

    fn delete(&mut self) {
        if self.cursor < self.text.len() {
            let end = self.next_boundary();
            self.text.drain(self.cursor..end);
        }
    }

    fn left(&mut self) {
        if self.cursor > self.line_start() {
            self.cursor = self.prev_boundary();
        }
    }

    fn right(&mut self) {
        if self.cursor < self.line_end() {
            self.cursor = self.next_boundary();
        }
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

    fn line_start(&self) -> usize {
        self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |i| self.cursor + i)
    }

    fn last_char_of_line(&self) -> usize {
        let end = self.line_end();
        self.text[self.line_start()..end]
            .char_indices()
            .next_back()
            .map_or(end, |(i, _)| self.line_start() + i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(draft: &mut Draft, keys: &[&str]) -> DraftKey {
        let mut last = DraftKey::Unhandled;
        for key in keys {
            last = match *key {
                "enter" | "escape" | "backspace" | "left" | "right" | "home" | "end" => {
                    draft.key(key, None, false)
                }
                text => draft.key(text, Some(text), false),
            };
        }
        last
    }

    #[test]
    fn insert_then_send_from_normal() {
        let mut draft = Draft::default();
        assert_eq!(
            keys(&mut draft, &["x"]),
            DraftKey::Changed,
            "normal: nothing to delete"
        );
        keys(&mut draft, &["i", "h", "i", "enter", "y", "o"]);
        assert_eq!(draft.text, "hi\nyo");
        assert_eq!(draft.mode, Mode::Insert);
        keys(&mut draft, &["escape"]);
        assert_eq!(draft.mode, Mode::Normal);
        assert_eq!(
            &draft.text[draft.cursor..],
            "o",
            "back onto the last character"
        );
        assert_eq!(
            keys(&mut draft, &["enter"]),
            DraftKey::Send("hi\nyo".into())
        );
        assert_eq!(draft, Draft::default());
        assert_eq!(
            keys(&mut draft, &["enter"]),
            DraftKey::Unhandled,
            "nothing to send"
        );
    }

    #[test]
    fn normal_keys_move_and_edit() {
        let mut draft = Draft::default();
        keys(&mut draft, &["i", "a", "b", "é", "escape", "0", "x"]);
        assert_eq!(draft.text, "bé");
        keys(&mut draft, &["$", "x"]);
        assert_eq!((draft.text.as_str(), draft.cursor), ("b", 0));
        keys(&mut draft, &["A", "c", "escape", "o", "d"]);
        assert_eq!(draft.text, "bc\nd");
        keys(&mut draft, &["backspace", "backspace", "backspace"]);
        assert_eq!(draft.text, "b");
        assert_eq!(draft.key("enter", None, true), DraftKey::Send("b".into()));
    }

    #[test]
    fn unknown_keys_pass_through() {
        let mut draft = Draft::default();
        assert_eq!(draft.key("j", Some("j"), false), DraftKey::Unhandled);
        assert_eq!(draft.key("f5", None, false), DraftKey::Unhandled);
        draft.mode = Mode::Insert;
        assert_eq!(draft.key("f5", None, false), DraftKey::Unhandled);
        assert_eq!(draft.key("k", Some("k"), false), DraftKey::Changed);
    }
}
