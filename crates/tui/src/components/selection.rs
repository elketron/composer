use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;

/// Text selected with the mouse, kept inside the pane it started in: from where the button
/// went down to where it is now, in reading order (as a terminal selects, but not across panes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    area: Rect,
    anchor: Position,
    head: Position,
}

impl Selection {
    pub fn start(area: Rect, at: Position) -> Self {
        let at = clamp(area, at);
        Self {
            area,
            anchor: at,
            head: at,
        }
    }

    /// The other end moves to `at`, held inside the pane.
    pub fn extend(&mut self, at: Position) {
        self.head = clamp(self.area, at);
    }

    /// A click, not a drag.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Shown reversed.
    pub fn highlight(&self, buf: &mut Buffer) {
        for (y, from, to) in self.rows() {
            for x in from..=to {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.modifier.toggle(Modifier::REVERSED);
                }
            }
        }
    }

    /// The selected text: a line per row, without the spaces that pad it out.
    pub fn text(&self, buf: &Buffer) -> String {
        let lines: Vec<String> = self
            .rows()
            .map(|(y, from, to)| {
                let mut line = String::new();
                for x in from..=to {
                    if let Some(cell) = buf.cell((x, y)) {
                        line.push_str(cell.symbol());
                    }
                }
                line.trim_end().to_string()
            })
            .collect();
        lines.join("\n").trim_matches('\n').to_string()
    }

    /// Each selected row and its first and last column.
    fn rows(&self) -> impl Iterator<Item = (u16, u16, u16)> + '_ {
        let (first, last) = if (self.anchor.y, self.anchor.x) <= (self.head.y, self.head.x) {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        };
        let (left, right) = (self.area.left(), self.area.right().saturating_sub(1));
        (first.y..=last.y).map(move |y| {
            let from = if y == first.y { first.x } else { left };
            let to = if y == last.y { last.x } else { right };
            (y, from, to)
        })
    }
}

fn clamp(area: Rect, at: Position) -> Position {
    Position {
        x: at.x.clamp(area.left(), area.right().saturating_sub(1)),
        y: at.y.clamp(area.top(), area.bottom().saturating_sub(1)),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::text::Line;
    use ratatui::widgets::Widget;

    use super::*;

    /// Two panes side by side, 10 columns each: "left" text on the left, "right" on the right.
    fn screen() -> Buffer {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 3));
        for (y, (l, r)) in [("one two", "uno"), ("three", "dos"), ("four", "tres")]
            .iter()
            .enumerate()
        {
            Line::from(*l).render(Rect::new(0, y as u16, 10, 1), &mut buf);
            Line::from(*r).render(Rect::new(10, y as u16, 10, 1), &mut buf);
        }
        buf
    }

    #[test]
    fn stays_in_its_pane() {
        let left = Rect::new(0, 0, 10, 3);
        let mut selection = Selection::start(left, Position::new(4, 0));
        // Dragged into the right pane and below the screen: held at the left pane's corner.
        selection.extend(Position::new(15, 9));
        assert_eq!(selection.text(&screen()), "two\nthree\nfour");
    }

    #[test]
    fn backwards_and_part_of_a_line() {
        let right = Rect::new(10, 0, 10, 3);
        let mut selection = Selection::start(right, Position::new(11, 2));
        selection.extend(Position::new(11, 1));
        assert_eq!(selection.text(&screen()), "os\ntr");
        assert!(!selection.is_empty());
        assert!(Selection::start(right, Position::new(12, 1)).is_empty());
    }

    #[test]
    fn highlights_only_what_it_selects() {
        let mut buf = screen();
        let mut selection = Selection::start(Rect::new(0, 0, 10, 3), Position::new(8, 0));
        selection.extend(Position::new(1, 1));
        selection.highlight(&mut buf);
        let reversed = |x, y| buf[(x, y)].modifier.contains(Modifier::REVERSED);
        assert!(reversed(8, 0) && reversed(9, 0) && reversed(0, 1) && reversed(1, 1));
        assert!(!reversed(7, 0) && !reversed(2, 1) && !reversed(10, 0));
    }
}
