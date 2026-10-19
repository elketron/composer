//! Which reminder pages are due, from the index (no IO).

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use composer_core::{Frontmatter, Index};

use crate::reminders::{DueReminder, ReminderAction};

/// Open, undelivered reminders due at or before `now` (local time), oldest first.
/// `due` is a date, optionally with a time (`2026-10-19 20:00`); `time:` sets the time
/// separately; without either, `default_time`. Pages with a `due` that doesn't parse are skipped.
pub fn due_reminders(
    index: &Index,
    now: NaiveDateTime,
    default_time: NaiveTime,
) -> Vec<DueReminder> {
    let mut due: Vec<DueReminder> = index
        .entries()
        .filter(|e| e.frontmatter.matches("type", "reminder"))
        .filter(|e| {
            let status = e.frontmatter.get_str("status").unwrap_or_default();
            status.is_empty() || status.eq_ignore_ascii_case("open")
        })
        .filter(|e| e.frontmatter.get("notified").is_none_or(|v| v.is_null()))
        .filter_map(|e| {
            let due_at = due_at(&e.frontmatter, default_time)?;
            (due_at <= now).then(|| DueReminder {
                url: e.url.clone(),
                title: e.title.clone(),
                due_at,
                action: action(&e.frontmatter),
            })
        })
        .collect();
    due.sort_by_key(|r| r.due_at);
    due
}

fn due_at(frontmatter: &Frontmatter, default_time: NaiveTime) -> Option<NaiveDateTime> {
    let due = frontmatter.get_str("due")?;
    let due = due.trim();
    let (date, time_in_due) = match due.split_once([' ', 'T']) {
        Some((date, time)) => (date, Some(time.trim())),
        None => (due, None),
    };
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let time = match frontmatter.get_str("time").filter(|t| !t.trim().is_empty()) {
        Some(time) => parse_time(&time)?,
        None => match time_in_due {
            Some(time) => parse_time(time)?,
            None => default_time,
        },
    };
    Some(date.and_time(time))
}

/// `20:00`, `20:00:00`, `8:30`.
pub(crate) fn parse_time(text: &str) -> Option<NaiveTime> {
    let text = text.trim();
    NaiveTime::parse_from_str(text, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(text, "%H:%M:%S"))
        .ok()
}

fn action(frontmatter: &Frontmatter) -> ReminderAction {
    match frontmatter.get_str("action").as_deref().map(str::trim) {
        Some("agent") => ReminderAction::Agent {
            session: frontmatter
                .get_str("session")
                .filter(|s| !s.trim().is_empty()),
            agent: frontmatter
                .get_str("agent")
                .filter(|a| !a.trim().is_empty())
                .unwrap_or_else(|| "default".into()),
        },
        _ => ReminderAction::Notify,
    }
}

/// The page with `notified: <stamp>` set in its frontmatter (replacing an empty one).
/// `None` if the page has no frontmatter.
pub fn mark_notified(raw: &str, stamp: &str) -> Option<String> {
    let newline = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let first = raw.find(newline)?;
    if raw[..first].trim_end() != "---" {
        return None;
    }
    let mut lines: Vec<&str> = raw.split(newline).collect();
    let close = lines.iter().skip(1).position(|l| l.trim_end() == "---")? + 1;
    let line = format!("notified: {stamp}");
    match (1..close).find(|&i| lines[i].starts_with("notified:")) {
        Some(i) => lines[i] = &line,
        None => lines.insert(close, &line),
    }
    Some(lines.join(newline))
}

#[cfg(test)]
mod tests {
    use composer_core::{Page, SiteUrl};

    use super::*;

    fn index(pages: &[(&str, &str)]) -> Index {
        let mut index = Index::default();
        for (path, frontmatter) in pages {
            let url = SiteUrl::new("calendar", path).unwrap();
            let raw = format!("---\ntitle: {path}\n{frontmatter}\n---\n# {path}\n");
            index.upsert(&Page::parse(url, raw));
        }
        index
    }

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap()
    }

    #[test]
    fn finds_what_is_due() {
        let index = index(&[
            (
                "evening.md",
                "type: reminder\ndue: 2026-10-19\ntime: \"20:00\"\nstatus: open",
            ),
            ("morning.md", "type: reminder\ndue: 2026-10-19"),
            ("inline.md", "type: reminder\ndue: 2026-10-19 12:30"),
            ("later.md", "type: reminder\ndue: 2026-10-20"),
            ("done.md", "type: reminder\ndue: 2026-10-01\nstatus: done"),
            (
                "sent.md",
                "type: reminder\ndue: 2026-10-01\nnotified: 2026-10-01 09:00",
            ),
            ("bad.md", "type: reminder\ndue: tonight"),
            ("note.md", "type: note\ndue: 2026-10-01"),
            (
                "ask.md",
                "type: reminder\ndue: 2026-10-19 08:00\naction: agent\nsession: ab12",
            ),
        ]);
        let nine = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        let names = |now| {
            due_reminders(&index, at(now), nine)
                .into_iter()
                .map(|r| r.url.path().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(names("2026-10-19 08:59"), ["ask.md"]);
        assert_eq!(
            names("2026-10-19 13:00"),
            ["ask.md", "morning.md", "inline.md"]
        );
        assert_eq!(names("2026-10-19 20:00").len(), 4);
        let ask = due_reminders(&index, at("2026-10-19 09:00"), nine).remove(0);
        assert_eq!(
            ask.action,
            ReminderAction::Agent {
                session: Some("ab12".into()),
                agent: "default".into()
            }
        );
    }

    #[test]
    fn notified_goes_into_the_frontmatter() {
        let page = "---\ntitle: x\ntype: reminder\n---\n# x\n\n---\nnot frontmatter\n";
        assert_eq!(
            mark_notified(page, "2026-10-19 20:00").unwrap(),
            "---\ntitle: x\ntype: reminder\nnotified: 2026-10-19 20:00\n---\n# x\n\n---\nnot frontmatter\n"
        );
        let empty = "---\ntitle: x\nnotified:\n---\n";
        assert_eq!(
            mark_notified(empty, "now").unwrap(),
            "---\ntitle: x\nnotified: now\n---\n"
        );
        assert_eq!(
            mark_notified("---\r\ntitle: x\r\n---\r\nbody", "now").unwrap(),
            "---\r\ntitle: x\r\nnotified: now\r\n---\r\nbody"
        );
        assert!(mark_notified("# no frontmatter\n", "now").is_none());
    }
}
