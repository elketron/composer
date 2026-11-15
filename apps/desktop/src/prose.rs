//! An agent's message split for drawing: prose, and fenced code (kept as written, in the grid
//! font). The rest of Markdown shows as typed, which reads well enough in a chat.

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Prose(String),
    Code(String),
}

pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            let done = std::mem::take(&mut current);
            let trimmed = done.trim_matches('\n');
            if !trimmed.is_empty() || in_code {
                out.push(if in_code {
                    Block::Code(trimmed.to_string())
                } else {
                    Block::Prose(trimmed.to_string())
                });
            }
            in_code = !in_code;
            continue;
        }
        current.push_str(line);
        current.push('\n');
    }
    let rest = current.trim_matches('\n');
    if !rest.is_empty() {
        out.push(if in_code {
            Block::Code(rest.to_string())
        } else {
            Block::Prose(rest.to_string())
        });
    }
    out
}

/// At most `max` lines of `text`, and how many were left out.
pub fn head(text: &str, max: usize) -> (String, usize) {
    let total = text.lines().count();
    let shown: Vec<&str> = text.lines().take(max).collect();
    (shown.join("\n"), total.saturating_sub(max))
}

/// A stable collapsed thinking preview: keep its beginning while later paragraphs stream in.
/// The growing word count indicates progress without replacing the line on every newline.
pub fn thinking_preview(text: &str) -> (&str, usize) {
    let text = text.trim();
    (
        text.lines().next().unwrap_or_default(),
        text.split_whitespace().count(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_preview_does_not_restart_when_new_lines_arrive() {
        let mut thinking = String::new();
        for (delta, words) in [
            ("Plan the scene.", 3),
            ("\n\n", 3),
            ("Check", 4),
            (" the objects.\n", 6),
            ("Then render.", 8),
        ] {
            thinking.push_str(delta);
            assert_eq!(thinking_preview(&thinking), ("Plan the scene.", words));
        }
        assert_eq!(thinking_preview("\n  "), ("", 0));
        assert_eq!(thinking_preview("\nPlan\r\nNext line"), ("Plan", 3));
    }

    #[test]
    fn fences_split_prose_and_code() {
        let text = "Here:\n\n```rust\nfn main() {}\n```\nDone.";
        assert_eq!(
            blocks(text),
            [
                Block::Prose("Here:".into()),
                Block::Code("fn main() {}".into()),
                Block::Prose("Done.".into()),
            ]
        );
        // Still streaming: an open fence is code so far.
        assert_eq!(
            blocks("a\n```\nb"),
            [Block::Prose("a".into()), Block::Code("b".into())]
        );
        assert_eq!(head("1\n2\n3", 2), ("1\n2".into(), 1));
    }
}
