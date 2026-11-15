//! What the workspace home lists (mockups/next 11, 12): sessions split into NEEDS YOU and
//! RECENT, projects, and their ages; plain data from the store and `GET /api/projects`.

use std::collections::BTreeMap;

use composer_client::Approval;
use composer_protocol::http::ProjectSummary;
use composer_protocol::{ApprovalKind, SessionInfo, TurnState};

use crate::pick_lists::project_name;

/// A session's row.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    pub id: String,
    pub agent: String,
    /// What waits for you (`approve run tools › greet`), else its state (`running`, `idle`).
    pub status: String,
    pub waiting: bool,
    pub preview: String,
    /// `site/project`.
    pub place: String,
    pub age: String,
}

/// A project's row.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectRow {
    pub name: String,
    /// `! 1 approval`, empty when nothing waits.
    pub waiting: String,
    /// `2 sessions · 1 running`.
    pub line: String,
    pub path: String,
    pub age: String,
}

/// The user's sessions (not workers'): those waiting for you, then the rest, each most recent
/// first.
pub fn session_rows(
    sessions: &[SessionInfo],
    approvals: &BTreeMap<String, Approval>,
    now: i64,
) -> (Vec<SessionRow>, Vec<SessionRow>) {
    let mut sessions: Vec<&SessionInfo> = sessions.iter().filter(|s| s.worker.is_none()).collect();
    sessions.sort_by(|a, b| b.updated.cmp(&a.updated));
    let (mut needs, mut recent) = (Vec::new(), Vec::new());
    for s in sessions {
        let asked = approvals.values().find(|a| a.session == s.id);
        let waiting = asked.is_some() || s.state == TurnState::AwaitingApproval;
        let status = match asked {
            Some(a) => format!("! {}", asked_line(a)),
            None if waiting => "! approval".into(),
            None => state_word(s.state).into(),
        };
        let row = SessionRow {
            id: s.id.clone(),
            agent: s.agent.clone(),
            status,
            waiting,
            preview: s.preview.clone().unwrap_or_default(),
            place: match (s.site.as_deref(), s.project.as_deref().map(project_name)) {
                (Some(site), Some(project)) => format!("{site}/{project}"),
                (Some(site), None) => site.to_string(),
                (None, Some(project)) => project.to_string(),
                (None, None) => String::new(),
            },
            age: epoch_secs(&s.updated)
                .map(|then| age(now, then))
                .unwrap_or_default(),
        };
        if waiting {
            needs.push(row);
        } else {
            recent.push(row);
        }
    }
    (needs, recent)
}

/// What an approval asks, in a few words.
pub fn asked_line(approval: &Approval) -> String {
    match approval.kind {
        ApprovalKind::Recipe => format!("approve run {} › {}", approval.site, approval.recipe),
        _ if !approval.title.is_empty() => format!("approve {}", approval.title),
        ApprovalKind::Merge => "approve merge".into(),
        ApprovalKind::Tool | ApprovalKind::Mcp => "approve a tool".into(),
        ApprovalKind::Publish => "approve publish".into(),
    }
}

pub fn state_word(state: TurnState) -> &'static str {
    match state {
        TurnState::Idle => "idle",
        TurnState::Running => "running",
        TurnState::Waiting => "waiting",
        TurnState::AwaitingApproval => "approval",
    }
}

pub fn project_rows(projects: &[ProjectSummary], now: i64) -> Vec<ProjectRow> {
    let mut rows: Vec<&ProjectSummary> = projects.iter().collect();
    // Used most recently first; never used ones by their files' age.
    let key = |p: &ProjectSummary| p.last_used.clone().unwrap_or_else(|| p.modified.clone());
    rows.sort_by_key(|p| std::cmp::Reverse(key(p)));
    rows.into_iter()
        .map(|p| {
            let plural =
                |n: u32, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
            let line = match (p.sessions, p.running) {
                (0, _) => "no sessions".to_string(),
                (n, 0) => plural(n, "session"),
                (n, r) => format!("{} · {r} running", plural(n, "session")),
            };
            ProjectRow {
                name: p.name.clone(),
                waiting: if p.waiting > 0 {
                    format!("! {}", plural(p.waiting, "approval"))
                } else {
                    String::new()
                },
                line,
                path: p.path.clone(),
                age: epoch_secs(&key(p))
                    .map(|then| age(now, then))
                    .unwrap_or_default(),
            }
        })
        .collect()
}

/// How long ago: `now`, `6m`, `3h`, `2d`, then the date (`Nov 11`).
pub fn age(now: i64, then: i64) -> String {
    let secs = (now - then).max(0);
    match secs {
        0..60 => "now".into(),
        60..3_600 => format!("{}m", secs / 60),
        3_600..86_400 => format!("{}h", secs / 3_600),
        86_400..604_800 => format!("{}d", secs / 86_400),
        _ => {
            const MONTHS: [&str; 12] = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            let (_, month, day) = civil_from_days(then.div_euclid(86_400));
            format!("{} {day}", MONTHS[(month - 1) as usize])
        }
    }
}

/// Seconds since the epoch of an RFC 3339 time (`2026-11-07T10:20:30.5+01:00`).
pub fn epoch_secs(text: &str) -> Option<i64> {
    let num = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let rest = &text[19..];
    let zone = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match zone {
        "Z" | "z" | "" => 0,
        z => {
            let sign = if z.starts_with('-') { -1 } else { 1 };
            let oh: i64 = z.get(1..3)?.parse().ok()?;
            let om: i64 = z.get(4..6)?.parse().ok()?;
            sign * (oh * 3_600 + om * 60)
        }
    };
    Some(days_from_civil(y, mo, d) * 86_400 + h * 3_600 + mi * 60 + s - offset)
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, updated: &str, state: &str) -> SessionInfo {
        serde_json::from_value(serde_json::json!({
            "id": id, "agent": "orchestrator", "project": "/home/u/Projects/game", "site": "3d",
            "ticket": null, "parent": null, "state": state, "last_seq": 0,
            "preview": "hello", "updated": updated
        }))
        .unwrap()
    }

    #[test]
    fn times() {
        let t = epoch_secs("2026-11-15T10:00:00Z").unwrap();
        assert_eq!(epoch_secs("2026-11-15T11:00:00.123+01:00"), Some(t));
        assert_eq!(epoch_secs("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(age(t + 30, t), "now");
        assert_eq!(age(t + 6 * 60, t), "6m");
        assert_eq!(age(t + 3 * 3_600, t), "3h");
        assert_eq!(age(t + 2 * 86_400, t), "2d");
        assert_eq!(age(t + 30 * 86_400, t), "Nov 15");
        assert_eq!(epoch_secs("yesterday"), None);
    }

    #[test]
    fn waiting_sessions_come_first() {
        let now = epoch_secs("2026-11-15T12:00:00Z").unwrap();
        let sessions = [
            session("a", "2026-11-15T11:00:00Z", "idle"),
            session("b", "2026-11-15T10:00:00Z", "idle"),
            session("c", "2026-11-15T11:30:00Z", "running"),
        ];
        let approval = Approval {
            request: "r".into(),
            session: "b".into(),
            kind: ApprovalKind::Recipe,
            title: String::new(),
            site: "tools".into(),
            recipe: "greet".into(),
            args: vec![],
            body: String::new(),
            rule: None,
        };
        let approvals = BTreeMap::from([("r".to_string(), approval)]);
        let (needs, recent) = session_rows(&sessions, &approvals, now);
        assert_eq!(needs.len(), 1);
        assert_eq!(needs[0].status, "! approve run tools › greet");
        assert_eq!(needs[0].place, "3d/game");
        assert_eq!(needs[0].age, "2h");
        let ids: Vec<&str> = recent.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["c", "a"]);
        assert_eq!(recent[0].status, "running");
    }

    #[test]
    fn projects_by_use() {
        let project = |name: &str, used: Option<&str>, sessions, running, waiting| ProjectSummary {
            name: name.into(),
            path: format!("~/Projects/{name}"),
            modified: "2026-11-01T00:00:00Z".into(),
            folder: String::new(),
            sessions,
            running,
            waiting,
            last_used: used.map(str::to_string),
            page: None,
        };
        let now = epoch_secs("2026-11-15T12:00:00Z").unwrap();
        let rows = project_rows(
            &[
                project("old", None, 0, 0, 0),
                project("game", Some("2026-11-15T11:00:00Z"), 2, 1, 1),
            ],
            now,
        );
        assert_eq!(rows[0].name, "game");
        assert_eq!(rows[0].line, "2 sessions · 1 running");
        assert_eq!(rows[0].waiting, "! 1 approval");
        assert_eq!(rows[1].line, "no sessions");
        assert_eq!(rows[1].age, "Nov 1");
    }
}
