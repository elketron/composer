//! Unified diff → display lines that know their file and line.

use composer_protocol::http::ProjectDiff;

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

/// The lines the changes view shows: per file the text diff's hunks, or (`semantic`, and the
/// server sent one for it) difft's structural diff.
pub fn view_lines(diff: &ProjectDiff, semantic: bool) -> Vec<DiffLine> {
    let text = parse_diff(&diff.diff);
    if !semantic {
        return text;
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let end = text[i + 1..]
            .iter()
            .position(|l| l.kind == DiffKind::File)
            .map_or(text.len(), |p| i + 1 + p);
        out.push(text[i].clone());
        let structural = text[i]
            .file
            .as_deref()
            .filter(|_| text[i].kind == DiffKind::File)
            .and_then(|file| diff.files.iter().find(|f| f.path == file))
            .and_then(|f| f.semantic.as_deref());
        match structural {
            Some(semantic) => out.extend(parse_semantic(text[i].file.as_deref(), semantic)),
            None => out.extend(text[i + 1..end].iter().cloned()),
        }
        i = end;
    }
    out
}

/// difft's plain output for one file: a header, then rows of a gutter (two line-number cells,
/// old and new, as wide as the biggest number and at least 2, and a space) and the code. An
/// unchanged row shows one number, a changed one the number on its side; plain output doesn't
/// say which rows were removed or added, so they are all context. A row anchors to the number
/// it shows (the new one when both).
pub fn parse_semantic(file: Option<&str>, semantic: &str) -> Vec<DiffLine> {
    let mut rows: Vec<&str> = semantic.lines().collect();
    if rows.first().is_some_and(|r| r.contains(" --- ")) {
        rows.remove(0);
    }
    let digits = |s: &str| s.trim().len();
    let mut width = 2;
    for row in &rows {
        let mut cells = row.split_whitespace();
        for cell in [cells.next(), cells.next()].into_iter().flatten() {
            if cell.chars().all(|c| c.is_ascii_digit()) {
                width = width.max(digits(cell));
            }
        }
    }
    rows.iter()
        .filter(|row| !row.trim().is_empty())
        .map(|row| {
            let cell = |from: usize, to: usize| -> Option<u32> {
                row.get(from..to.min(row.len()))
                    .and_then(|c| c.trim().parse().ok())
            };
            let number = cell(width, 2 * width).or_else(|| cell(0, width));
            DiffLine {
                kind: if number.is_some() {
                    DiffKind::Context
                } else {
                    DiffKind::Note
                },
                text: row.get(2 * width + 1..).unwrap_or("").to_string(),
                file: file.map(str::to_string),
                line: number,
            }
        })
        .collect()
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

    const DIFFT: &str = "a.rs --- Rust\n1    fn a() {\n2        let x = 1;\n   2     let x = 2;\n   4     bar();\n   5 }\n";

    #[test]
    fn semantic_rows_anchor_to_the_number_their_gutter_shows() {
        let rows = parse_semantic(Some("a.rs"), DIFFT);
        let shown: Vec<(Option<u32>, &str)> =
            rows.iter().map(|l| (l.line, l.text.as_str())).collect();
        assert_eq!(
            shown,
            [
                (Some(1), "fn a() {"),
                (Some(2), "    let x = 1;"),
                (Some(2), "    let x = 2;"),
                (Some(4), "    bar();"),
                (Some(5), "}"),
            ]
        );
        let wide = "b.py --- Python\n 98    l98\n100    l100\n   100 changed\n";
        let shown: Vec<_> = parse_semantic(None, wide)
            .iter()
            .map(|l| (l.line, l.text.clone()))
            .collect();
        assert_eq!(
            shown,
            [
                (Some(98), "l98".into()),
                (Some(100), "l100".into()),
                (Some(100), "changed".into())
            ]
        );
    }

    fn project_diff(semantic: Option<&str>) -> ProjectDiff {
        use composer_protocol::http::ChangedFile;
        let file = |path: &str, semantic: Option<&str>| ChangedFile {
            path: path.into(),
            status: "M".into(),
            added: 1,
            removed: 1,
            semantic: semantic.map(str::to_string),
        };
        ProjectDiff {
            path: "/wt".into(),
            branch: None,
            files: vec![file("a.rs", semantic), file("n.txt", None)],
            diff: "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n fn a() {\n-    let x = 1;\n+    let x = 2;\ndiff --git a/n.txt b/n.txt\n--- a/n.txt\n+++ b/n.txt\n@@ -1 +1 @@\n-a\n+b\n".into(),
            ticket: None,
        }
    }

    #[test]
    fn the_structural_diff_replaces_only_the_files_that_have_one() {
        let diff = project_diff(Some(DIFFT));
        let semantic = view_lines(&diff, true);
        let hunks = |lines: &[DiffLine], file: &str| {
            lines
                .iter()
                .filter(|l| l.kind == DiffKind::Hunk && l.file.as_deref() == Some(file))
                .count()
        };
        assert_eq!(hunks(&semantic, "a.rs"), 0, "a.rs shows difft's rows");
        assert_eq!(
            hunks(&semantic, "n.txt"),
            1,
            "n.txt falls back to its text diff"
        );
        assert!(semantic.iter().any(|l| l.text == "    bar();"));
        let text = view_lines(&diff, false);
        assert_eq!(hunks(&text, "a.rs"), 1);
        assert!(!text.iter().any(|l| l.text == "    bar();"));
        // Without a semantic diff from the server, both views are the text diff.
        let none = project_diff(None);
        assert_eq!(view_lines(&none, true), view_lines(&none, false));
    }
}
