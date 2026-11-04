//! One displayed row of text: a byte range that can never run backwards.

use unicode_width::UnicodeWidthChar;

/// One displayed row of text (a hard line or a wrapped segment of one), as the byte range
/// start..end in the whole text. The invariant the constructor keeps: start <= end, so
/// slicing text by a row can never run backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Row {
    start: usize,
    end: usize,
}

impl Row {
    /// The range start..end, or None if it would run backwards.
    pub(crate) fn new(start: usize, end: usize) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }

    pub(crate) fn end(&self) -> usize {
        self.end
    }

    /// The row's own text: its range without the hard newline a whole line holds.
    /// An empty row (the one after a text's trailing newline) has empty content.
    pub(crate) fn content<'a>(&self, text: &'a str) -> &'a str {
        let raw = &text[self.start..self.end];
        raw.strip_suffix('\n').unwrap_or(raw)
    }

    /// Whether the row ends at a hard newline (its range holds it).
    pub(crate) fn hard_break(&self, text: &str) -> bool {
        self.content(text).len() < self.end - self.start
    }

    /// Whether a cursor offset (a byte position in text) rests on this row; the last
    /// row also holds the cursor at its end.
    pub(crate) fn holds_cursor(&self, cursor: usize, last: bool) -> bool {
        cursor >= self.start && (cursor < self.end || last)
    }

    /// The displayed column of a cursor offset resting on this row.
    pub(crate) fn column_of(&self, text: &str, cursor: usize) -> usize {
        text[self.start..cursor]
            .chars()
            .filter_map(|c| c.width())
            .sum()
    }

    /// The cursor offset on this row at displayed column col (clamped to its content):
    /// after the last character that fits, or the row's start if it has none.
    pub(crate) fn offset_at_column(&self, text: &str, col: usize) -> usize {
        let mut used = 0;
        let mut pos = self.start;
        for (i, c) in self.content(text).char_indices() {
            let w = c.width().unwrap_or(0);
            if used + w > col {
                break;
            }
            used += w;
            pos = self.start + i + c.len_utf8();
        }
        pos
    }

    /// The offset of the row's last displayed character, or its start when it has none.
    pub(crate) fn last_char_offset(&self, text: &str) -> usize {
        self.start
            + self
                .content(text)
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_that_runs_backwards_is_no_row() {
        assert_eq!(Row::new(4, 5), Some(Row { start: 4, end: 5 }));
        assert_eq!(
            Row::new(5, 5),
            Some(Row { start: 5, end: 5 }),
            "an empty row is a row"
        );
        assert_eq!(Row::new(5, 4), None, "the invariant the type keeps");
    }

    #[test]
    fn content_drops_only_the_rows_own_newline() {
        let text = "abcd\nef\nx";
        let line = Row::new(0, 5).unwrap();
        assert_eq!(line.content(text), "abcd");
        assert!(line.hard_break(text));
        let second = Row::new(5, 8).unwrap();
        assert_eq!(second.content(text), "ef");
        let last = Row::new(8, 9).unwrap();
        assert_eq!(last.content(text), "x");
        assert!(!last.hard_break(text));
    }

    #[test]
    fn an_empty_row_holds_the_cursor_at_its_start_and_measures_zero() {
        let text = "abcd\n";
        let row = Row::new(5, 5).unwrap();
        assert!(row.holds_cursor(5, true));
        assert_eq!(row.column_of(text, 5), 0);
        assert_eq!(
            row.offset_at_column(text, 3),
            5,
            "nothing to land on: its start"
        );
        assert_eq!(row.last_char_offset(text), 5);
    }
}
