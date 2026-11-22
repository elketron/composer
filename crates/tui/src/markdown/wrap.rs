use unicode_width::UnicodeWidthStr;

use crate::markdown::types::Seg;

/// Greedy word wrap that keeps styles and link ids.
pub fn wrap(segs: Vec<Seg>, width: usize, first: &[Seg], rest: &[Seg]) -> Vec<Vec<Seg>> {
    let mut lines = Vec::new();
    let mut line: Vec<Seg> = first.to_vec();
    let mut used: usize = first.iter().map(|s| s.text.width()).sum();
    let mut at_start = true;
    for seg in segs {
        for token in tokens(&seg.text) {
            let is_space = token.chars().all(char::is_whitespace);
            if is_space && at_start {
                continue;
            }
            let w = token.width();
            if used + w > width && !at_start {
                lines.push(std::mem::take(&mut line));
                line = rest.to_vec();
                used = rest.iter().map(|s| s.text.width()).sum();
                at_start = true;
                if is_space {
                    continue;
                }
            }
            let mut token = token.to_string();
            // A word longer than the line: hard-break it.
            while used + token.width() > width && token.chars().count() > 1 {
                let room = width.saturating_sub(used).max(1);
                let head = truncate(&token, room);
                let tail = token[head.len()..].to_string();
                line.push(Seg {
                    text: head,
                    style: seg.style,
                    link: seg.link,
                });
                lines.push(std::mem::take(&mut line));
                line = rest.to_vec();
                used = rest.iter().map(|s| s.text.width()).sum();
                token = tail;
            }
            used += token.width();
            line.push(Seg {
                text: token,
                style: seg.style,
                link: seg.link,
            });
            at_start = false;
        }
    }
    if !at_start || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// Splits into alternating word and whitespace tokens.
fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = None;
    for (i, c) in text.char_indices() {
        let space = c.is_whitespace();
        if in_space.is_some_and(|s| s != space) {
            out.push(&text[start..i]);
            start = i;
        }
        in_space = Some(space);
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// Word-wraps a line of styled spans to `width` columns, keeping each span's style. Rows after the
/// first start with `rest` (a hanging indent or a bar). Always returns at least one row.
pub fn wrap_spans(
    spans: Vec<ratatui::text::Span<'static>>,
    width: usize,
    rest: Vec<ratatui::text::Span<'static>>,
) -> Vec<ratatui::text::Line<'static>> {
    let seg = |span: ratatui::text::Span<'static>| Seg {
        text: span.content.into_owned(),
        style: span.style,
        link: None,
    };
    let segs: Vec<Seg> = spans.into_iter().map(seg).collect();
    let rest: Vec<Seg> = rest.into_iter().map(seg).collect();
    wrap(segs, width.max(1), &[], &rest)
        .into_iter()
        .map(|row| {
            ratatui::text::Line::from(
                row.into_iter()
                    .map(|s| ratatui::text::Span::styled(s.text, s.style))
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// Breaks `text` into rows of at most `width` columns, by character, keeping all whitespace (for
/// code and command output, where words are not the unit). An empty line stays one empty row.
pub fn hard_wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut rest = text;
    loop {
        let head = truncate(rest, width);
        // A character wider than the whole row still has to go somewhere.
        let head = if head.is_empty() {
            rest.chars().next().map(String::from).unwrap_or_default()
        } else {
            head
        };
        rest = &rest[head.len()..];
        rows.push(head);
        if rest.is_empty() {
            return rows;
        }
    }
}

/// The longest prefix of `text` that fits in `width` columns.
pub fn truncate(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > width {
            break;
        }
        used += w;
        out.push(c);
    }
    out
}

#[cfg(test)]
mod line_tests {
    use super::*;
    use ratatui::style::Style;
    use ratatui::text::Span;

    fn text(line: &ratatui::text::Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn spans_wrap_with_a_hanging_prefix_and_keep_their_styles() {
        let bold = Style::new().bold();
        let lines = wrap_spans(
            vec![
                Span::styled("system-1", bold),
                Span::raw(" forwarded a long command to you"),
            ],
            16,
            vec![Span::raw("  ")],
        );
        let rows: Vec<String> = lines
            .iter()
            .map(|l| text(l).trim_end().to_string())
            .collect();
        assert_eq!(
            rows,
            ["system-1", "  forwarded a", "  long command", "  to you"]
        );
        assert_eq!(lines[0].spans[0].style, bold);
        assert!(rows.iter().all(|r| r.chars().count() <= 16));
    }

    #[test]
    fn a_short_line_stays_one_row() {
        let lines = wrap_spans(vec![Span::raw("ok")], 40, vec![]);
        assert_eq!(lines.len(), 1);
        assert_eq!(text(&lines[0]), "ok");
    }

    #[test]
    fn hard_wrap_keeps_every_character() {
        let line = "    let x = some_function(argument_one, argument_two);";
        let rows = hard_wrap(line, 20);
        assert!(rows.iter().all(|r| r.len() <= 20));
        assert_eq!(rows.concat(), line);
        assert_eq!(hard_wrap("", 10), [""]);
        assert_eq!(
            hard_wrap("日本語テキスト", 4),
            ["日本", "語テ", "キス", "ト"]
        );
    }
}
