//! What the home lists (mockups/next 11): sessions split into NEEDS YOU and RECENT, and their
//! ages; plain data from the store.

use std::collections::BTreeMap;

use composer_client::Approval;
use composer_protocol::{ApprovalKind, SessionInfo, TurnState};

use crate::pick_lists::kind_label;

/// A session's row.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    pub id: String,
    /// Its run kind (`agentic`, `workflow`, `single shot`).
    pub kind: String,
    /// What waits for you (`approve run tools › greet`), else its state (`running`, `idle`).
    pub status: String,
    pub waiting: bool,
    pub preview: String,
    /// Its site, empty for none.
    pub place: String,
    pub age: String,
}

/// The sessions: those waiting for you, then the rest, each most recent first.
pub fn session_rows(
    sessions: &[SessionInfo],
    approvals: &BTreeMap<String, Approval>,
    now: i64,
) -> (Vec<SessionRow>, Vec<SessionRow>) {
    let mut sessions: Vec<&SessionInfo> = sessions.iter().collect();
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
            kind: kind_label(s),
            status,
            waiting,
            preview: s.preview.clone().unwrap_or_default(),
            place: s.site.clone().unwrap_or_default(),
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
        ApprovalKind::Recipe if is_mcp_start(approval) => {
            format!("approve: {}", mcp_start(&approval.site))
        }
        ApprovalKind::Recipe if is_folder(approval) => {
            format!("approve: {}", folder_use(&approval.site, &approval.args))
        }
        ApprovalKind::Recipe => format!("approve run {} › {}", approval.site, approval.recipe),
        _ if !approval.title.is_empty() => format!("approve {}", approval.title),
        ApprovalKind::Tool | ApprovalKind::Mcp => "approve a tool".into(),
    }
}

/// The approval to start a site's MCP servers (its `mcp.toml`), not a recipe run.
pub fn is_mcp_start(approval: &Approval) -> bool {
    approval.kind == ApprovalKind::Recipe && approval.recipe == MCP_FILE
}

/// What approving a site's `mcp.toml` does.
pub fn mcp_start(site: &str) -> String {
    format!("start {site}'s MCP servers")
}

/// The file a site declares its MCP servers in; approving it starts them.
pub const MCP_FILE: &str = "mcp.toml";

/// What a site's `folder:` approval is asked under; its one argument is the folder.
pub const FOLDER: &str = "folder:";

/// The approval to let a site work in the folder its `folder:` names, not a recipe run.
pub fn is_folder(approval: &Approval) -> bool {
    approval.kind == ApprovalKind::Recipe && approval.recipe == FOLDER
}

/// What approving a site's `folder:` does: "Let <site> work in <path>".
pub fn folder_use(site: &str, args: &[String]) -> String {
    format!(
        "Let {site} work in {}",
        args.first().map(String::as_str).unwrap_or("its folder")
    )
}

pub fn state_word(state: TurnState) -> &'static str {
    match state {
        TurnState::Idle => "idle",
        TurnState::Running => "running",
        TurnState::Waiting => "waiting",
        TurnState::AwaitingApproval => "approval",
    }
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
            "id": id, "kind": "agentic", "site": "3d",
            "parent": null, "state": state, "last_seq": 0,
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
        assert_eq!(needs[0].place, "3d");
        assert_eq!(needs[0].age, "2h");
        let ids: Vec<&str> = recent.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["c", "a"]);
        assert_eq!(recent[0].status, "running");
    }
}
