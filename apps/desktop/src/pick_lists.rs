//! What the pickers list: sessions (most recent first), the palette's commands, models.

use std::collections::BTreeMap;

use composer_client::Approval;
use composer_protocol::http::SiteSummary;
use composer_protocol::{ClientCommand, ModelInfo, ProcessInfo, RunKind, SessionInfo};

use crate::home_lists::{age, epoch_secs};
use crate::picker::{Item, Pick};

/// The run kinds a new session can be, the default first.
pub const RUN_KINDS: [RunKind; 3] = [RunKind::Agentic, RunKind::Workflow, RunKind::SingleShot];

/// A run kind in a few words.
pub fn kind_name(kind: RunKind) -> &'static str {
    match kind {
        RunKind::Agentic => "agentic",
        RunKind::Workflow => "workflow",
        RunKind::SingleShot => "single shot",
    }
}

/// A session's run kind, and the outside harness that runs it if one does.
pub fn kind_label(session: &SessionInfo) -> String {
    match &session.harness {
        Some(harness) => format!("{} · {harness}", kind_name(session.kind)),
        None => kind_name(session.kind).to_string(),
    }
}

/// The command that starts a session of `kind`, in `site` or none, run by `harness` or
/// composer; the server's model.
pub fn new_session(kind: RunKind, site: Option<String>, harness: Option<String>) -> ClientCommand {
    ClientCommand::SessionCreate {
        kind,
        model: None,
        site,
        harness,
    }
}

/// The new-session menu: each run kind in the shown session's site (or none), then each kind
/// without a site, then in each other site of `sites`. An agentic session is offered run by
/// composer, then by each of the server's `harnesses`.
pub fn new_sessions(
    sites: &[SiteSummary],
    current: Option<&str>,
    harnesses: &[String],
) -> Vec<Item> {
    let mut places: Vec<Option<&str>> = vec![current];
    if current.is_some() {
        places.push(None);
    }
    places.extend(
        sites
            .iter()
            .map(|s| s.name.as_str())
            .filter(|name| Some(*name) != current)
            .map(Some),
    );
    places
        .into_iter()
        .flat_map(|site| {
            RUN_KINDS
                .into_iter()
                .flat_map(move |kind| {
                    let others: &[String] = if kind == RunKind::Agentic {
                        harnesses
                    } else {
                        &[]
                    };
                    std::iter::once(None)
                        .chain(others.iter().map(Some))
                        .map(move |harness| (kind, harness))
                })
                .map(move |(kind, harness)| Item {
                    label: match harness {
                        Some(h) => format!("{} session · {h}", kind_name(kind)),
                        None => format!("{} session", kind_name(kind)),
                    },
                    detail: site.map_or("no site".into(), |s| format!("in {s}")),
                    note: String::new(),
                    group: "new session",
                    pick: Pick::CreateSession {
                        kind,
                        site: site.map(str::to_string),
                        harness: harness.cloned(),
                    },
                })
        })
        .collect()
}

/// The sessions, most recent first: the run kind and id, whether it is shown or waits for an
/// approval, its site, and what it last said underneath.
pub fn sessions(
    all: &[SessionInfo],
    shown: Option<&str>,
    waiting: &BTreeMap<String, Approval>,
) -> Vec<Item> {
    let mut sessions: Vec<&SessionInfo> = all.iter().collect();
    sessions.sort_by(|a, b| b.updated.cmp(&a.updated));
    sessions
        .into_iter()
        .map(|s| {
            let mark = if shown == Some(s.id.as_str()) {
                " · shown"
            } else if waiting.values().any(|w| w.session == s.id) {
                " ! waiting"
            } else {
                ""
            };
            Item {
                label: format!("{} #{}{mark}", kind_label(s), s.id),
                note: s
                    .preview
                    .as_deref()
                    .map(|p| short(p, 80))
                    .unwrap_or_default(),
                detail: s.site.clone().unwrap_or_else(|| "no site".into()),
                group: "sessions",
                pick: Pick::Session(s.id.clone()),
            }
        })
        .collect()
}

pub fn commands(following: bool) -> Vec<Item> {
    let follow = if following { "off" } else { "on" };
    vec![
        Item {
            label: "Switch session".into(),
            detail: "ctrl+shift+s".into(),
            note: String::new(),
            group: "session",
            pick: Pick::Sessions,
        },
        Item {
            label: "New session".into(),
            detail: "agentic, workflow or single shot".into(),
            note: String::new(),
            group: "session",
            pick: Pick::NewSession,
        },
        Item {
            label: "Attach a file…".into(),
            detail: "or drop it on the chat".into(),
            note: String::new(),
            group: "session",
            pick: Pick::AttachFile,
        },
        Item {
            label: format!("Follow agent: turn {follow}"),
            detail: String::new(),
            note: String::new(),
            group: "session",
            pick: Pick::ToggleFollow,
        },
        Item {
            label: "Model…".into(),
            detail: "for this session's turns".into(),
            note: String::new(),
            group: "session",
            pick: Pick::ModelMenu,
        },
        Item {
            label: "Compact the conversation".into(),
            detail: "save memory, summarize older messages".into(),
            note: String::new(),
            group: "session",
            pick: Pick::Compact,
        },
        Item {
            label: "Delete session…".into(),
            detail: "this one, with its folder".into(),
            note: String::new(),
            group: "session",
            pick: Pick::DeleteMenu,
        },
        Item {
            label: "Background processes…".into(),
            detail: "the session's; open or stop one".into(),
            note: String::new(),
            group: "session",
            pick: Pick::ProcessMenu,
        },
        Item {
            label: "Open terminal tab".into(),
            detail: "+".into(),
            note: String::new(),
            group: "stage",
            pick: Pick::NewTerminal,
        },
        Item {
            label: "Sessions overview".into(),
            detail: "ctrl+shift+h".into(),
            note: String::new(),
            group: "home",
            pick: Pick::Home,
        },
    ]
}

/// The confirm for deleting `session`: one item; Escape keeps it.
pub fn delete_confirm(session: &SessionInfo) -> Vec<Item> {
    vec![Item {
        label: format!("Delete {} #{}", kind_label(session), session.id),
        detail: "can't be undone".into(),
        note: session
            .preview
            .as_deref()
            .map(|p| short(p, 80))
            .unwrap_or_default(),
        group: "delete session?",
        pick: Pick::DeleteSession(session.id.clone()),
    }]
}

/// The shown session's background processes: open each one's terminal on the stage, or stop it.
/// `now`: seconds since the epoch, for how long each has run.
pub fn processes(processes: &[ProcessInfo], now: i64) -> Vec<Item> {
    processes
        .iter()
        .flat_map(|p| {
            let since = match epoch_secs(&p.started).map(|then| age(now, then)) {
                Some(age) if age == "now" => "just started".to_string(),
                Some(age) => format!("running {age}"),
                None => String::new(),
            };
            [
                Item {
                    label: format!("open: {}", short(&p.command, 60)),
                    detail: since.clone(),
                    note: p.cwd.clone(),
                    group: "processes",
                    pick: Pick::OpenProcess(p.terminal.clone()),
                },
                Item {
                    label: format!("stop: {}", short(&p.command, 60)),
                    detail: since,
                    note: String::new(),
                    group: "processes",
                    pick: Pick::StopProcess(p.terminal.clone()),
                },
            ]
        })
        .collect()
}

/// "leave site" while the shown session is in one.
pub fn leave_site(current: Option<&str>) -> Vec<Item> {
    current
        .map(|site| Item {
            label: "leave site".into(),
            detail: format!("leave {site}, keep the conversation"),
            note: String::new(),
            group: "site",
            pick: Pick::LeaveSite,
        })
        .into_iter()
        .collect()
}

/// One "site: <name>" per site, with the one the session is in marked.
pub fn sites(sites: &[SiteSummary], current: Option<&str>) -> Vec<Item> {
    sites
        .iter()
        .map(|site| {
            let mut about = Vec::new();
            if !site.mcp.is_empty() {
                about.push(format!("mcp {}", site.mcp.join(", ")));
            }
            if current == Some(site.name.as_str()) {
                about.push("◂ here".into());
            }
            Item {
                label: format!("site: {}", site.name),
                detail: about.join(" · "),
                note: String::new(),
                group: "site",
                pick: Pick::Site(site.name.clone()),
            }
        })
        .collect()
}

/// The provider's models for the shown session; first, the server's default.
pub fn models(models: &[ModelInfo], default: &str, current: Option<&str>) -> Vec<Item> {
    let mark = |id: Option<&str>| if current == id { " · shown" } else { "" };
    let mut items = vec![Item {
        label: format!("Server default{}", mark(None)),
        detail: default.to_string(),
        note: String::new(),
        group: "models",
        pick: Pick::SetModel(None),
    }];
    items.extend(models.iter().map(|m| {
        Item {
            label: format!("{}{}", m.id, mark(Some(&m.id))),
            detail: m
                .context
                .map(|c| format!("{}k context", c / 1000))
                .unwrap_or_default(),
            note: m.name.clone().unwrap_or_default(),
            group: "models",
            pick: Pick::SetModel(Some(m.id.clone())),
        }
    }));
    items
}

/// At most `max` characters, an ellipsis when cut.
pub fn short(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, updated: &str, preview: Option<&str>) -> SessionInfo {
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": "workflow", "site": "game",
            "parent": null, "state": "idle", "last_seq": 0, "preview": preview, "updated": updated
        }))
        .unwrap()
    }

    #[test]
    fn sessions_recent_first() {
        let all = [
            session("a", "2026-11-01T00:00:00Z", Some("old one")),
            session("b", "2026-11-07T00:00:00Z", None),
        ];
        let approval = Approval {
            request: "r1".into(),
            session: "b".into(),
            kind: Default::default(),
            title: String::new(),
            site: "tools".into(),
            recipe: "greet".into(),
            args: vec![],
            body: String::new(),
            rule: None,
        };
        let waiting = BTreeMap::from([("r1".to_string(), approval)]);
        let items = sessions(&all, Some("a"), &waiting);
        let picks: Vec<&Pick> = items.iter().map(|i| &i.pick).collect();
        assert_eq!(
            picks,
            [&Pick::Session("b".into()), &Pick::Session("a".into())]
        );
        assert_eq!(items[0].label, "workflow #b ! waiting");
        assert_eq!(items[0].note, "", "no preview: nothing under it");
        assert_eq!(items[1].label, "workflow #a · shown");
        assert_eq!(items[1].note, "old one");
        assert_eq!(items[1].detail, "game");
        assert!(items.iter().all(|i| i.group == "sessions"));
    }

    fn site(name: &str, mcp: &[&str]) -> SiteSummary {
        serde_json::from_value(serde_json::json!({
            "name": name, "title": name, "has_justfile": false, "mcp": mcp
        }))
        .unwrap()
    }

    #[test]
    fn a_palette_entry_per_site_and_a_way_out() {
        let sites = [
            site("3d", &["blender"]),
            site("notes", &[]),
            site("research", &["wayai", "web"]),
        ];
        let items = super::sites(&sites, Some("3d"));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["site: 3d", "site: notes", "site: research"]);
        assert_eq!(items[0].pick, Pick::Site("3d".into()));
        assert_eq!(items[0].detail, "mcp blender · ◂ here");
        assert_eq!(items[1].detail, "");
        assert_eq!(items[2].detail, "mcp wayai, web");
        assert!(leave_site(None).is_empty());
        let leave = leave_site(Some("3d"));
        assert_eq!(leave[0].label, "leave site");
        assert_eq!(leave[0].pick, Pick::LeaveSite);
    }

    #[test]
    fn commands_and_text() {
        assert_eq!(commands(true)[3].label, "Follow agent: turn off");
        assert_eq!(commands(true)[1].pick, Pick::NewSession);
        assert_eq!(commands(true)[1].label, "New session");
        assert_eq!(commands(true)[2].pick, Pick::AttachFile);
        let groups: Vec<&str> = commands(false).iter().map(|i| i.group).collect();
        assert_eq!(
            groups,
            [
                "session", "session", "session", "session", "session", "session", "session",
                "session", "stage", "home"
            ],
            "grouped as shown"
        );
        assert_eq!(short("abcdef", 3), "abc…");
        assert_eq!(short("abc", 3), "abc");
        assert_eq!(short("héllo wörld", 4), "héll…", "by characters");
    }

    #[test]
    fn models() {
        let model = ModelInfo {
            id: "m1".into(),
            name: Some("Model One".into()),
            context: Some(128_000),
        };
        let items = super::models(&[model], "m0", Some("m1"));
        assert_eq!(items[0].label, "Server default");
        assert_eq!(items[0].detail, "m0");
        assert_eq!(items[1].label, "m1 · shown");
        assert_eq!(items[1].detail, "128k context");
    }

    #[test]
    fn a_new_session_has_a_kind_and_maybe_a_site() {
        for site in [Some("game".to_string()), None] {
            match new_session(RunKind::Workflow, site.clone(), None) {
                ClientCommand::SessionCreate {
                    kind,
                    model,
                    site: s,
                    harness,
                } => {
                    assert_eq!(kind, RunKind::Workflow);
                    assert_eq!(model, None);
                    assert_eq!(s, site);
                    assert_eq!(harness, None);
                }
                other => panic!("not a session create: {other:?}"),
            }
        }
    }

    #[test]
    fn the_new_session_menu_starts_agentic_in_the_current_site() {
        let sites = [site("3d", &[]), site("notes", &[])];
        let items = new_sessions(&sites, Some("3d"), &[]);
        assert_eq!(items.len(), 9, "3d, no site, notes: three kinds each");
        assert_eq!(items[0].label, "agentic session");
        assert_eq!(items[0].detail, "in 3d");
        assert_eq!(
            items[0].pick,
            Pick::CreateSession {
                kind: RunKind::Agentic,
                site: Some("3d".into()),
                harness: None,
            }
        );
        assert_eq!(items[1].label, "workflow session");
        assert_eq!(items[2].label, "single shot session");
        assert_eq!(items[3].detail, "no site");
        assert_eq!(items[6].detail, "in notes");
        let none = new_sessions(&sites, None, &[]);
        assert_eq!(none.len(), 9, "no site, 3d, notes");
        assert_eq!(
            none[0].pick,
            Pick::CreateSession {
                kind: RunKind::Agentic,
                site: None,
                harness: None,
            }
        );
    }

    #[test]
    fn harnesses_run_agentic_sessions_only() {
        let harnesses = ["claude".to_string()];
        let items = new_sessions(&[], None, &harnesses);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "agentic session",
                "agentic session · claude",
                "workflow session",
                "single shot session"
            ]
        );
        assert_eq!(
            items[1].pick,
            Pick::CreateSession {
                kind: RunKind::Agentic,
                site: None,
                harness: Some("claude".into()),
            }
        );
        let mut s = session("x", "2026-11-01T00:00:00Z", None);
        assert_eq!(kind_label(&s), "workflow");
        s.kind = RunKind::Agentic;
        s.harness = Some("claude".into());
        assert_eq!(kind_label(&s), "agentic · claude");
        assert_eq!(delete_confirm(&s)[0].label, "Delete agentic · claude #x");
        assert_eq!(delete_confirm(&s)[0].pick, Pick::DeleteSession("x".into()));
    }

    #[test]
    fn a_process_opens_or_stops() {
        let process = ProcessInfo {
            terminal: "t7".into(),
            command: "cargo run".into(),
            cwd: "/w".into(),
            started: "2026-11-21T10:00:00Z".into(),
        };
        let now = epoch_secs("2026-11-21T10:05:00Z").unwrap();
        let items = processes(&[process], now);
        assert_eq!(items[0].label, "open: cargo run");
        assert_eq!(items[0].detail, "running 5m");
        assert_eq!(items[0].pick, Pick::OpenProcess("t7".into()));
        assert_eq!(items[1].label, "stop: cargo run");
        assert_eq!(items[1].pick, Pick::StopProcess("t7".into()));
    }
}
