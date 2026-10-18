//! Tickets in `site://orchestration/tickets/<id>.md`.

use composer_core::{CoreError, SiteUrl};

/// `3`, `0003`, `0003.md` or `site://orchestration/tickets/0003` → `0003`.
pub fn normalize_id(raw: &str) -> String {
    let id = raw
        .trim()
        .trim_start_matches("site://")
        .trim_start_matches("orchestration/")
        .trim_start_matches("tickets/")
        .trim_end_matches(".md");
    match id.parse::<u32>() {
        Ok(n) => format!("{n:04}"),
        Err(_) => id.to_string(),
    }
}

pub fn ticket_url(id: &str) -> Result<SiteUrl, CoreError> {
    SiteUrl::new("orchestration", &format!("tickets/{id}.md"))
}

/// Sets `status:` in the frontmatter (adding it if missing). Leaves the rest untouched.
pub fn set_status(raw: &str, status: &str) -> String {
    let Some(rest) = raw.strip_prefix("---\n") else {
        return format!("---\nstatus: {status}\n---\n{raw}");
    };
    let Some(end) = rest.find("\n---") else {
        return raw.to_string();
    };
    let (front, body) = rest.split_at(end);
    let mut found = false;
    let lines: Vec<String> = front
        .lines()
        .map(|line| {
            if line.starts_with("status:") {
                found = true;
                // Keep a trailing comment like `# open | claimed | done | failed`.
                let comment = line.find(" #").map(|i| &line[i..]).unwrap_or("");
                format!("status: {status}{comment}")
            } else {
                line.to_string()
            }
        })
        .collect();
    let mut front = lines.join("\n");
    if !found {
        front.push_str(&format!("\nstatus: {status}"));
    }
    format!("---\n{front}{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert_eq!(normalize_id("3"), "0003");
        assert_eq!(normalize_id("0012.md"), "0012");
        assert_eq!(normalize_id("site://orchestration/tickets/0007"), "0007");
    }

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
    }
}
