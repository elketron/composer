//! Tickets in `site://orchestration/tickets/<id>.md`.

use composer_core::{CoreError, SiteUrl};
use composer_protocol::ChangedBy;

use crate::server::Server;

pub use crate::worker::ticket_id::TicketId;

/// The ticket's page, `site://orchestration/tickets/<id>.md`.
pub fn ticket_url(id: &TicketId) -> Result<SiteUrl, CoreError> {
    SiteUrl::new("orchestration", &format!("tickets/{id}.md"))
}

/// Sets frontmatter fields of ticket `id` (a harness recording its session, model, exit).
pub fn set_ticket_fields(
    server: &Server,
    id: &TicketId,
    fields: &[(&str, &str)],
) -> Result<(), String> {
    let url = ticket_url(id).map_err(|e| e.to_string())?;
    let page = server.web.read_page(&url).map_err(|e| e.to_string())?;
    let raw = fields.iter().fold(page.raw.clone(), |raw, (key, value)| {
        set_field(&raw, key, value)
    });
    server
        .write_page(&url, &raw, ChangedBy::Agent)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Sets `status:` in the frontmatter (adding it if missing). Leaves the rest untouched.
pub fn set_status(raw: &str, status: &str) -> String {
    set_field(raw, "status", status)
}

/// Sets `key:` in the frontmatter (adding it if missing), keeping a trailing `# comment`.
pub fn set_field(raw: &str, key: &str, value: &str) -> String {
    let Some(rest) = raw.strip_prefix("---\n") else {
        return format!("---\n{key}: {value}\n---\n{raw}");
    };
    let Some(end) = rest.find("\n---") else {
        return raw.to_string();
    };
    let (front, body) = rest.split_at(end);
    let mut found = false;
    let lines: Vec<String> = front
        .lines()
        .map(|line| {
            if line.starts_with(&format!("{key}:")) {
                found = true;
                // Keep a trailing comment like `# open | claimed | done | failed`.
                let comment = line.find(" #").map(|i| &line[i..]).unwrap_or("");
                format!("{key}: {value}{comment}")
            } else {
                line.to_string()
            }
        })
        .collect();
    let mut front = lines.join("\n");
    if !found {
        front.push_str(&format!("\n{key}: {value}"));
    }
    format!("---\n{front}{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {}

    #[test]
    fn status() {
        let raw = "---\ntitle: X\nstatus: open          # open | claimed\nagent: worker\n---\n# X\nstatus: body text\n";
        assert_eq!(
            set_status(raw, "claimed"),
            "---\ntitle: X\nstatus: claimed # open | claimed\nagent: worker\n---\n# X\nstatus: body text\n"
        );
        assert_eq!(
            set_status("---\ntitle: X\n---\nbody", "done"),
            "---\ntitle: X\nstatus: done\n---\nbody"
        );
        assert_eq!(
            set_field(
                "---\ntitle: X\nbranch: old\n---\nbody",
                "branch",
                "ticket/0001"
            ),
            "---\ntitle: X\nbranch: ticket/0001\n---\nbody"
        );
    }
}
