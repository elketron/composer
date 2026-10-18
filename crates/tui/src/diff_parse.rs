//! Unified diff → display lines that know their file and line.

use crate::store::types::{DiffKind, DiffLine};

pub fn parse_diff(diff: &str) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut old_file: Option<String> = None;
    let mut file: Option<String> = None;
    let mut new_line = 0u32;
    let line = |kind, text: &str, file: &Option<String>, line: Option<u32>| DiffLine {
        kind,
        text: text.to_string(),
        file: file.clone(),
        line,
    };
    for raw in diff.lines() {
        if raw.starts_with("diff --git") {
            old_file = None;
            file = None;
        } else if let Some(path) = raw.strip_prefix("--- ") {
            old_file = path.strip_prefix("a/").map(str::to_string);
        } else if let Some(path) = raw.strip_prefix("+++ ") {
            file = path
                .strip_prefix("b/")
                .map(str::to_string)
                .or_else(|| old_file.clone());
            let label = if path == "/dev/null" {
                format!("{} (deleted)", file.as_deref().unwrap_or("?"))
            } else {
                file.clone().unwrap_or_default()
            };
            lines.push(line(DiffKind::File, &label, &file, Some(1)));
        } else if raw.starts_with("@@") {
            // @@ -a,b +c,d @@ context
            new_line = raw
                .split_whitespace()
                .find_map(|p| p.strip_prefix('+'))
                .and_then(|p| p.split(',').next())
                .and_then(|n| n.parse().ok())
                .unwrap_or(1);
            lines.push(line(DiffKind::Hunk, raw, &file, Some(new_line.max(1))));
        } else if let Some(text) = raw.strip_prefix('+') {
            lines.push(line(DiffKind::Add, text, &file, Some(new_line)));
            new_line += 1;
        } else if let Some(text) = raw.strip_prefix('-') {
            lines.push(line(DiffKind::Del, text, &file, Some(new_line.max(1))));
        } else if let Some(text) = raw.strip_prefix(' ') {
            lines.push(line(DiffKind::Context, text, &file, Some(new_line)));
            new_line += 1;
        } else if raw.starts_with('\\') || raw.starts_with('…') {
            lines.push(line(DiffKind::Note, raw, &file, None));
        }
        // index, mode and "new file" lines are not shown.
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_files_and_new_line_numbers() {
        let diff = "diff --git a/src/x.rs b/src/x.rs\nindex 1..2 100644\n--- a/src/x.rs\n+++ b/src/x.rs\n@@ -10,3 +10,4 @@ fn x\n ctx\n-old\n+new\n+more\n tail\ndiff --git a/gone.rs b/gone.rs\n--- a/gone.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-bye\n";
        let lines = parse_diff(diff);
        let shown: Vec<(DiffKind, &str, Option<u32>)> = lines
            .iter()
            .map(|l| (l.kind, l.text.as_str(), l.line))
            .collect();
        assert_eq!(
            shown,
            [
                (DiffKind::File, "src/x.rs", Some(1)),
                (DiffKind::Hunk, "@@ -10,3 +10,4 @@ fn x", Some(10)),
                (DiffKind::Context, "ctx", Some(10)),
                (DiffKind::Del, "old", Some(11)),
                (DiffKind::Add, "new", Some(11)),
                (DiffKind::Add, "more", Some(12)),
                (DiffKind::Context, "tail", Some(13)),
                (DiffKind::File, "gone.rs (deleted)", Some(1)),
                (DiffKind::Hunk, "@@ -1 +0,0 @@", Some(1)),
                (DiffKind::Del, "bye", Some(1)),
            ]
        );
        assert_eq!(lines[4].file.as_deref(), Some("src/x.rs"));
        assert_eq!(lines[9].file.as_deref(), Some("gone.rs"));
    }
}
