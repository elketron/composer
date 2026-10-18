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
