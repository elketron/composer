//! Review comments on a ticket's changes, written onto the ticket page.

use composer_protocol::ChangedBy;
use composer_protocol::http::DiffComment;

use crate::server::Server;
use crate::worker::ticket::ticket_url;

const HEADING: &str = "## Comments";
/// A quote longer than this isn't repeated in the comment line.
const QUOTE_MAX_CHARS: usize = 80;

/// Appends `comments` to the ticket page under `## Comments` (created at the end when missing),
/// one `- file:line — message` line each, through the server's page write. Returns every line of
/// that section as the page has it now.
pub fn ticket_comments(
    server: &Server,
    ticket: &str,
    comments: &[DiffComment],
) -> Result<Vec<String>, String> {
    let url = ticket_url(ticket).map_err(|e| e.to_string())?;
    let page = server
        .workspace
        .read_page(&url)
        .map_err(|e| e.to_string())?;
    let raw = if comments.is_empty() {
        page.raw
    } else {
        let raw = append_comments(
            &page.raw,
            &comments.iter().map(comment_line).collect::<Vec<_>>(),
        );
        server
            .write_page(&url, &raw, ChangedBy::Agent)
            .map_err(|e| e.to_string())?;
        raw
    };
    Ok(comment_lines(&raw))
}

/// `- file:line — message`, with the quoted line (`file:line — \`quote\`: message`) when it is a
/// single short line, so a reader of the ticket alone knows what was meant.
pub fn comment_line(comment: &DiffComment) -> String {
    let message = comment.message.trim().replace('\n', " ");
    let quote = comment
        .quote
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty() && !q.contains('\n') && !q.contains('`'))
        .filter(|q| q.chars().count() <= QUOTE_MAX_CHARS);
    match quote {
        Some(q) => format!("- {}:{} — `{q}`: {message}", comment.file, comment.line),
        None => format!("- {}:{} — {message}", comment.file, comment.line),
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
