//! Small edits to index pages: a row in a section's table, an item in a section's list.
//! Sections are found by their `## ` heading; nothing else on the page changes.

/// The lines of the section under `## <heading>` (case-insensitive), as a range of line indexes
/// after the heading line, up to the next `#`/`##` heading.
fn section(lines: &[&str], heading: &str) -> Option<(usize, usize)> {
    let wanted = heading.to_lowercase();
    let start = lines.iter().position(|line| {
        line.strip_prefix("## ")
            .is_some_and(|h| h.trim().to_lowercase() == wanted)
    })? + 1;
    let end = lines[start..]
        .iter()
        .position(|line| line.starts_with("# ") || line.starts_with("## "))
        .map_or(lines.len(), |i| start + i);
    Some((start, end))
}

fn insert(text: &str, at: usize, new: &[String]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines.splice(at..at, new.iter().cloned());
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Adds `row` after the last row of the first table in the section. `None` if there is no such
/// section or it has no table.
pub fn append_table_row(text: &str, heading: &str, row: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let (start, end) = section(&lines, heading)?;
    let first = (start..end).find(|&i| lines[i].trim_start().starts_with('|'))?;
    let last = (first..end)
        .take_while(|&i| lines[i].trim_start().starts_with('|'))
        .last()?;
    Some(insert(text, last + 1, &[row.to_string()]))
}

/// Adds `- <item>` after the section's last list item, or as its first one.
/// `None` if there is no such section.
pub fn append_list_item(text: &str, heading: &str, item: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let (start, end) = section(&lines, heading)?;
    let item = format!("- {item}");
    match (start..end).rev().find(|&i| lines[i].starts_with("- ")) {
        Some(last) => Some(insert(text, last + 1, &[item])),
        None => {
            // Right after the section's text, keeping a blank line before the next heading.
            let mut at = end;
            while at > start && lines[at - 1].trim().is_empty() {
                at -= 1;
            }
            let mut new = vec![String::new(), item];
            if end < lines.len() {
                new.push(String::new());
            }
            if at == start {
                new.remove(0);
                if lines.get(start).is_some_and(|l| l.trim().is_empty()) {
                    at += 1;
                } else {
                    new.insert(0, String::new());
                }
            }
            Some(insert(text, at, &new))
        }
    }
}

/// Adds a whole section (`## <heading>` and `body`) before `## <before>`, or at the end.
pub fn add_section(text: &str, heading: &str, body: &str, before: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut new = vec![format!("## {heading}"), String::new()];
    new.extend(body.trim_end().lines().map(str::to_string));
    match section(&lines, before) {
        Some((start, _)) => {
            new.push(String::new());
            insert(text, start - 1, &new)
        }
        None => {
            let mut out = text.trim_end().to_string();
            out.push_str("\n\n");
            out.push_str(&new.join("\n"));
            out.push('\n');
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "# Memory\n\n## Registry\n\n| project | path |\n|---|---|\n| [a](projects/a.md) | `~/a` |\n\nText after.\n\n## Pages\n\n- [user](user.md)\n- [a](projects/a.md)\n";

    #[test]
    fn rows_go_after_the_last_row() {
        let out = append_table_row(PAGE, "registry", "| [b](projects/b.md) | `~/b` |").unwrap();
        assert!(
            out.contains("| `~/a` |\n| [b](projects/b.md) | `~/b` |\n\nText after."),
            "{out}"
        );
        assert!(
            append_table_row(PAGE, "Pages", "| x |").is_none(),
            "no table there"
        );
        assert!(
            append_table_row(PAGE, "Sites", "| x |").is_none(),
            "no such section"
        );
    }

    #[test]
    fn items_go_after_the_last_item() {
        let out = append_list_item(PAGE, "Pages", "[b](projects/b.md)").unwrap();
        assert!(
            out.ends_with("- [a](projects/a.md)\n- [b](projects/b.md)\n"),
            "{out}"
        );
        let empty = "# Site\n\n## Pages\n\n## Rules\n\n- x\n";
        assert_eq!(
            append_list_item(empty, "Pages", "[p](p.md)").unwrap(),
            "# Site\n\n## Pages\n\n- [p](p.md)\n\n## Rules\n\n- x\n"
        );
        assert_eq!(
            append_list_item("# Site\n\n## Pages\n", "Pages", "[p](p.md)").unwrap(),
            "# Site\n\n## Pages\n\n- [p](p.md)\n"
        );
    }

    #[test]
    fn sections_go_before_another() {
        let out = add_section(PAGE, "Projects", "| a |\n|---|", "Pages");
        assert!(
            out.contains("Text after.\n\n## Projects\n\n| a |\n|---|\n\n## Pages\n"),
            "{out}"
        );
        let out = add_section("# X\n", "Projects", "body", "Pages");
        assert_eq!(out, "# X\n\n## Projects\n\nbody\n");
    }
}
