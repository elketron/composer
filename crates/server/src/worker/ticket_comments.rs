//! Review comments on a ticket's changes, written onto the ticket page.

use std::sync::Mutex;

use composer_protocol::ChangedBy;
use composer_protocol::http::DiffComment;

use crate::server::Server;
use crate::worker::ticket::ticket_url;

const HEADING: &str = "## Comments";
/// A quote longer than this isn't repeated in the comment line.
const QUOTE_MAX_CHARS: usize = 80;
/// Comment sends read the page, add lines and write it back: one at a time, so two clients
/// sending together don't lose each other's lines. (The page write itself has no lock or
/// compare-and-set, so an agent or editor writing in between can still win.)
static SEND: Mutex<()> = Mutex::new(());

/// Appends `comments` to the ticket page under `## Comments` (created at the end when missing),
/// one `- file:start:end — message` line each, through the server's page write. Returns every line of
/// that section as the page has it now.
pub fn ticket_comments(
    server: &Server,
    ticket: &str,
    comments: &[DiffComment],
) -> Result<Vec<String>, String> {
    let url = ticket_url(ticket).map_err(|e| e.to_string())?;
    let _sending = SEND.lock().unwrap_or_else(|e| e.into_inner());
    let page = server.web.read_page(&url).map_err(|e| e.to_string())?;
    let raw = if comments.is_empty() {
        page.raw
    } else {
        let raw = append_comments(
            &page.raw,
            &comments.iter().map(comment_line).collect::<Vec<_>>(),
        );
        server
            .write_page(&url, &raw, ChangedBy::App)
            .map_err(|e| e.to_string())?;
        raw
    };
    Ok(comment_lines(&raw))
}

/// `- file:start:end — message`, with the quoted line (`file:start:end — \`quote\`: message`) when it is a
/// single short line, so a reader of the ticket alone knows what was meant.
pub fn comment_line(comment: &DiffComment) -> String {
    let message = comment.message.trim().replace('\n', " ");
    let quote = comment
        .quote
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty() && !q.contains('\n') && !q.contains('`'))
        .filter(|q| q.chars().count() <= QUOTE_MAX_CHARS);
    let anchor = format!("{}:{}:{}", comment.file, comment.start, comment.end);
    match quote {
        Some(q) => format!("- {anchor} — `{q}`: {message}"),
        None => format!("- {anchor} — {message}"),
    }
}

/// Adds `lines` to the end of the `## Comments` section, or a new section at the end of the page.
pub fn append_comments(raw: &str, lines: &[String]) -> String {
    let added = lines.join("\n");
    let all: Vec<&str> = raw.lines().collect();
    let Some(start) = all.iter().position(|l| l.trim_end() == HEADING) else {
        let mut out = raw.trim_end().to_string();
        out.push_str(&format!("\n\n{HEADING}\n\n{added}\n"));
        return out;
    };
    let end = all[start + 1..]
        .iter()
        .position(|l| l.starts_with("## ") || l.starts_with("# "))
        .map_or(all.len(), |i| start + 1 + i);
    // Insert after the section's last non-blank line.
    let mut at = end;
    while at > start + 1 && all[at - 1].trim().is_empty() {
        at -= 1;
    }
    let mut out: Vec<String> = all[..at].iter().map(|l| l.to_string()).collect();
    if at == start + 1 {
        out.push(String::new());
    }
    out.extend(lines.iter().cloned());
    if at < end {
        out.push(String::new());
    }
    out.extend(all[end..].iter().map(|l| l.to_string()));
    let mut text = out.join("\n");
    text.push('\n');
    text
}

/// The `- …` lines of the `## Comments` section.
pub fn comment_lines(raw: &str) -> Vec<String> {
    let mut in_section = false;
    let mut lines = Vec::new();
    for line in raw.lines() {
        if line.trim_end() == HEADING {
            in_section = true;
        } else if in_section && (line.starts_with("## ") || line.starts_with("# ")) {
            break;
        } else if in_section && line.starts_with("- ") {
            lines.push(line.trim_end().to_string());
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(start: u32, end: u32, quote: Option<&str>) -> DiffComment {
        DiffComment {
            file: "a.rs".into(),
            start,
            end,
            message: "why?".into(),
            quote: quote.map(str::to_string),
        }
    }

    #[test]
    fn a_single_line_is_start_equals_end() {
        assert_eq!(comment_line(&comment(2, 2, None)), "- a.rs:2:2 — why?");
        assert_eq!(
            comment_line(&comment(2, 2, Some("let x;"))),
            "- a.rs:2:2 — `let x;`: why?"
        );
    }

    #[test]
    fn a_range_keeps_both_ends_and_drops_a_multi_line_quote() {
        assert_eq!(
            comment_line(&comment(2, 5, Some("a\nb"))),
            "- a.rs:2:5 — why?"
        );
    }

    #[test]
    fn a_removed_line_quote_is_trimmed_with_its_marker() {
        // The quote both clients build for `-    let x = 1;` (their tests pin the same string).
        assert_eq!(
            comment_line(&comment(2, 2, Some("-let x = 1;"))),
            "- a.rs:2:2 — `-let x = 1;`: why?"
        );
    }
}
