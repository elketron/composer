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
}
