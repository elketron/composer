//! A terminal pane's grid without the pane: where a pixel lands, what is selected, how far back
//! the view is scrolled. Rows are absolute (0 = the oldest line kept), so a selection stays on its
//! text while output scrolls.

/// The grid's place and cell size, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub left: f32,
    pub top: f32,
    pub cell_w: f32,
    pub line_h: f32,
    pub cols: usize,
    pub rows: usize,
}

impl Geometry {
    /// The visible cell (column, row) under a point, clamped to the grid.
    pub fn cell_at(&self, x: f32, y: f32) -> (usize, usize) {
        let col = ((x - self.left) / self.cell_w).floor().max(0.0) as usize;
        let row = ((y - self.top) / self.line_h).floor().max(0.0) as usize;
        (
            col.min(self.cols.saturating_sub(1)),
            row.min(self.rows.saturating_sub(1)),
        )
    }
}

/// How far the view is scrolled back from the live screen, in lines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Scrollback(usize);

impl Scrollback {
    pub fn lines(self) -> usize {
        self.0
    }

    /// Scrolled by `delta` lines (positive: back in time), within the `kept` lines of history.
    pub fn scrolled(self, delta: i64, kept: usize) -> Self {
        Self((self.0 as i64 + delta).clamp(0, kept as i64) as usize)
    }

    /// The absolute row at the top of the view: history is `kept` lines long.
    pub fn top_row(self, kept: usize) -> usize {
        kept - self.0.min(kept)
    }
}

/// A selection from where the drag started to where it is, in absolute (row, column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    anchor: (usize, usize),
    head: (usize, usize),
}

impl Selection {
    pub fn at(row: usize, col: usize) -> Self {
        Self {
            anchor: (row, col),
            head: (row, col),
        }
    }

    pub fn to(self, row: usize, col: usize) -> Self {
        Self {
            head: (row, col),
            ..self
        }
    }

    /// Start and end, in reading order (end inclusive).
    pub fn ordered(self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub fn is_empty(self) -> bool {
        self.anchor == self.head
    }

    pub fn contains(self, row: usize, col: usize) -> bool {
        let (start, end) = self.ordered();
        (row, col) >= start && (row, col) <= end
    }

    /// The selected text of `lines` (each the text of absolute row `first + i`): whole rows in
    /// between, trailing blanks trimmed, rows joined with newlines.
    pub fn text(self, first: usize, lines: &[String]) -> String {
        let ((r0, c0), (r1, c1)) = self.ordered();
        let mut out = Vec::new();
        for row in r0..=r1 {
            let Some(line) = row.checked_sub(first).and_then(|i| lines.get(i)) else {
                continue;
            };
            let chars: Vec<char> = line.chars().collect();
            let from = if row == r0 { c0 } else { 0 };
            let to = if row == r1 {
                (c1 + 1).min(chars.len())
            } else {
                chars.len()
            };
            let part: String = chars
                .get(from.min(to)..to)
                .unwrap_or_default()
                .iter()
                .collect();
            out.push(part.trim_end().to_string());
        }
        out.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_under_points() {
        let g = Geometry {
            left: 10.0,
            top: 20.0,
            cell_w: 8.0,
            line_h: 18.0,
            cols: 80,
            rows: 24,
        };
        assert_eq!(g.cell_at(10.0, 20.0), (0, 0));
        assert_eq!(g.cell_at(17.9, 37.9), (0, 0));
        assert_eq!(g.cell_at(18.0, 38.0), (1, 1));
        assert_eq!(g.cell_at(0.0, 0.0), (0, 0), "left of and above the grid");
        assert_eq!(g.cell_at(9999.0, 9999.0), (79, 23), "past it");
    }

    #[test]
    fn scrolling_back_and_down() {
        let s = Scrollback::default().scrolled(5, 100);
        assert_eq!(s.lines(), 5);
        assert_eq!(s.top_row(100), 95);
        assert_eq!(s.scrolled(-10, 100).lines(), 0, "not past the live screen");
        assert_eq!(
            s.scrolled(500, 100).lines(),
            100,
            "not past the oldest line"
        );
        assert_eq!(
            Scrollback::default().scrolled(3, 0).lines(),
            0,
            "no history"
        );
    }

    #[test]
    fn selections_both_ways_and_their_text() {
        let lines = vec![
            "hello world   ".to_string(),
            "second line".to_string(),
            "third".to_string(),
        ];
        let s = Selection::at(10, 6).to(11, 5);
        assert!(s.contains(10, 6) && s.contains(10, 40) && s.contains(11, 0) && s.contains(11, 5));
        assert!(!s.contains(10, 5) && !s.contains(11, 6));
        assert_eq!(s.text(10, &lines), "world\nsecond");
        let back = Selection::at(11, 5).to(10, 6);
        assert_eq!(back.ordered(), s.ordered(), "dragged upwards");
        assert_eq!(Selection::at(12, 0).to(12, 99).text(10, &lines), "third");
        assert!(Selection::at(1, 1).is_empty());
        assert_eq!(
            Selection::at(3, 0).to(3, 4).text(10, &lines),
            "",
            "rows not kept"
        );
    }
}
