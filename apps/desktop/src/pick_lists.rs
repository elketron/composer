//! What the pickers list: workspaces, sessions (most recent first), the palette's commands.

use std::collections::BTreeMap;

use composer_client::Approval;
use composer_protocol::http::ProjectSummary;
use composer_protocol::http::SiteSummary;
use composer_protocol::{ClientCommand, ModelInfo, SessionInfo, WorkerState};

use crate::picker::{Item, Pick};

/// The agent every session the desktop starts runs as: the user talks to the orchestrator, which
/// spawns the workers. The desktop never asks for an agent.
pub const ORCHESTRATOR: &str = "orchestrator";

/// The command that starts a session in `project` (a folder) or none.
pub fn new_session(project: Option<String>) -> ClientCommand {
    ClientCommand::SessionCreate {
        agent: ORCHESTRATOR.into(),
        project,
    }
}

pub fn workspaces(names: Vec<String>, this: &str) -> Vec<Item> {
    names
        .into_iter()
        .enumerate()
        .map(|(i, name)| Item {
            detail: if i < 9 {
                format!("ctrl+alt+{}", i + 1)
            } else {
                String::new()
            },
            label: if name == this {
                format!("{name}  (this window)")
            } else {
                name.clone()
            },
            note: String::new(),
            group: "workspace",
            pick: Pick::Workspace(name),
        })
        .collect()
}

/// The user's sessions (not workers'), most recent first: the agent and id, whether it is shown
/// or waits for an approval, its site and project, and what it last said underneath. Then the
/// shown orchestration's workers.
pub fn sessions(
    all: &[SessionInfo],
    shown: Option<&str>,
    waiting: &BTreeMap<String, Approval>,
) -> Vec<Item> {
    let mut sessions: Vec<&SessionInfo> = all.iter().filter(|s| s.worker.is_none()).collect();
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
            let project = s
                .project
                .as_deref()
                .map(project_name)
                .unwrap_or("no project");
            Item {
                label: format!("{} #{}{mark}", s.agent, s.id),
                note: s
                    .preview
                    .as_deref()
                    .map(|p| short(p, 80))
                    .unwrap_or_default(),
                detail: match &s.site {
                    Some(site) => format!("{site}/{project}"),
                    None => project.to_string(),
                },
                group: "sessions",
                pick: Pick::Session(s.id.clone()),
            }
        })
        .chain(workers(all, shown, waiting))
        .collect()
}

/// The workers of the shown session's orchestration (its own, or its orchestrator's when a
/// worker is shown), most recent first: the way into a worker's conversation.
fn workers(
    sessions: &[SessionInfo],
    shown: Option<&str>,
    waiting: &BTreeMap<String, Approval>,
) -> Vec<Item> {
    let Some(shown) = shown else {
        return Vec::new();
    };
    let root = sessions
        .iter()
        .find(|s| s.id == shown)
        .and_then(|s| s.parent.as_deref())
        .unwrap_or(shown);
    let mut workers: Vec<&SessionInfo> = sessions
        .iter()
        .filter(|s| s.worker.is_some() && s.parent.as_deref() == Some(root))
        .collect();
    workers.sort_by(|a, b| b.updated.cmp(&a.updated));
    workers
        .into_iter()
        .map(|s| {
            let mark = if shown == s.id {
                " · shown"
            } else if waiting.values().any(|w| w.session == s.id) {
                " ! waiting"
            } else {
                ""
            };
            let state = match s.worker {
                Some(WorkerState::Starting) => "starting",
                Some(WorkerState::Running) => "running",
                Some(WorkerState::Done) => "done",
                Some(WorkerState::Failed) => "failed",
                Some(WorkerState::Paused) => "paused",
                Some(WorkerState::Stuck) => "stuck",
                None => "",
            };
            Item {
                label: format!("worker #{}{mark}", s.id),
                note: s
                    .preview
                    .as_deref()
                    .map(|p| short(p, 80))
                    .unwrap_or_default(),
                detail: match &s.ticket {
                    Some(ticket) => format!("ticket {ticket} · {state}"),
                    None => state.to_string(),
                },
                group: "workers",
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
            detail: "as the orchestrator, pick a project".into(),
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
            label: "Set project…".into(),
            detail: "what the session works in".into(),
            note: String::new(),
            group: "session",
            pick: Pick::ProjectMenu,
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
            label: "Open terminal tab".into(),
            detail: "+".into(),
            note: String::new(),
            group: "stage",
            pick: Pick::NewTerminal,
        },
        Item {
            label: "Switch workspace".into(),
            detail: "ctrl+shift+w".into(),
            note: String::new(),
            group: "workspace",
            pick: Pick::Workspaces,
        },
        Item {
            label: "Sessions overview".into(),
            detail: "ctrl+shift+h".into(),
            note: String::new(),
            group: "workspace",
            pick: Pick::Home,
        },
        Item {
            label: "Projects".into(),
            detail: String::new(),
            note: String::new(),
            group: "workspace",
            pick: Pick::Projects,
        },
    ]
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
            if let Some(app) = &site.app {
                about.push(format!("app {app}"));
            }
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

/// The projects to put the shown session in (most recently used first), and none.
pub fn projects(projects: &[ProjectSummary], current: Option<&str>) -> Vec<Item> {
    project_items(projects, current, "the sites only", Pick::SetProject)
}

/// The projects a new session can start in (most recently used first), and none.
pub fn new_session_projects(projects: &[ProjectSummary]) -> Vec<Item> {
    project_items(
        projects,
        None,
        "a session on the sites only",
        Pick::NewSessionIn,
    )
}

fn project_items(
    projects: &[ProjectSummary],
    current: Option<&str>,
    none: &str,
    pick: fn(Option<String>) -> Pick,
) -> Vec<Item> {
    let mut sorted: Vec<&ProjectSummary> = projects.iter().collect();
    sorted.sort_by_key(|p| std::cmp::Reverse(p.last_used.clone().unwrap_or_default()));
    let mut items: Vec<Item> = sorted
        .into_iter()
        .map(|p| Item {
            label: if current == Some(p.folder.as_str()) {
                format!("{} · shown", p.name)
            } else {
                p.name.clone()
            },
            detail: p.path.clone(),
            note: String::new(),
            group: "projects",
            pick: pick(Some(p.folder.clone())),
        })
        .collect();
    items.push(Item {
        label: "No project".into(),
        detail: none.into(),
        note: String::new(),
        group: "projects",
        pick: pick(None),
    });
    items
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

/// A project's name from its folder.
pub fn project_name(folder: &str) -> &str {
    folder
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, updated: &str, preview: Option<&str>, worker: bool) -> SessionInfo {
        let mut json = serde_json::json!({
            "id": id, "agent": "orchestrator", "project": "/home/u/Projects/game/", "ticket": null,
            "parent": null, "state": "idle", "last_seq": 0, "preview": preview, "updated": updated
        });
        if worker {
            json["worker"] = serde_json::json!("running");
        }
        serde_json::from_value(json).unwrap()
    }

    fn worker(id: &str, parent: &str, ticket: &str, updated: &str) -> SessionInfo {
        let mut s = session(id, updated, Some("on it"), true);
        s.agent = "coder".into();
        s.parent = Some(parent.into());
        s.ticket = Some(ticket.into());
        s
    }

    #[test]
    fn the_shown_orchestrations_workers_come_after_the_sessions() {
        let all = [
            session("o", "2026-11-01T00:00:00Z", None, false),
            session("p", "2026-11-02T00:00:00Z", None, false),
            worker("w1", "o", "0201", "2026-11-03T00:00:00Z"),
            worker("w2", "o", "0202", "2026-11-04T00:00:00Z"),
            worker("x", "p", "0300", "2026-11-05T00:00:00Z"),
        ];
        let none = BTreeMap::new();
        let picks = |shown| -> Vec<Pick> {
            sessions(&all, Some(shown), &none)
                .into_iter()
                .map(|i| i.pick)
                .collect()
        };
        let s = |id: &str| Pick::Session(id.into());
        assert_eq!(picks("o"), [s("p"), s("o"), s("w2"), s("w1")]);
        assert_eq!(
            picks("w1"),
            [s("p"), s("o"), s("w2"), s("w1")],
            "a shown worker: its orchestrator's workers"
        );
        let items = sessions(&all, Some("w1"), &none);
        assert_eq!(items[3].label, "worker #w1 · shown");
        assert_eq!(items[2].detail, "ticket 0202 · running");
        assert_eq!(items[2].note, "on it");
        assert_eq!(items[2].group, "workers");
        assert_eq!(
            sessions(&all, None, &none).len(),
            2,
            "nothing shown: no workers"
        );
    }

    #[test]
    fn sessions_recent_first_without_workers() {
        let all = [
            session("a", "2026-11-01T00:00:00Z", Some("old one"), false),
            session("b", "2026-11-07T00:00:00Z", None, false),
            session("w", "2026-11-08T00:00:00Z", None, true),
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
        assert_eq!(items[0].label, "orchestrator #b ! waiting");
        assert_eq!(items[0].note, "", "no preview: nothing under it");
        assert_eq!(items[1].label, "orchestrator #a · shown");
        assert_eq!(items[1].note, "old one");
        assert_eq!(items[1].detail, "game");
        assert!(items.iter().all(|i| i.group == "sessions"));
    }

    fn site(name: &str, app: Option<&str>, mcp: &[&str]) -> SiteSummary {
        serde_json::from_value(serde_json::json!({
            "name": name, "title": name, "has_justfile": false, "has_ui": false,
            "app": app, "mcp": mcp
        }))
        .unwrap()
    }

    #[test]
    fn a_palette_entry_per_site_and_a_way_out() {
        let sites = [
            site("3d", Some("blender"), &["blender"]),
            site("notes", None, &[]),
            site("research", None, &["wayai", "web"]),
        ];
        let items = super::sites(&sites, Some("3d"));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["site: 3d", "site: notes", "site: research"]);
        assert_eq!(items[0].pick, Pick::Site("3d".into()));
        assert_eq!(items[0].detail, "app blender · mcp blender · ◂ here");
        assert_eq!(items[1].detail, "");
        assert_eq!(items[2].detail, "mcp wayai, web");
        assert!(leave_site(None).is_empty());
        let leave = leave_site(Some("3d"));
        assert_eq!(leave[0].label, "leave site");
        assert_eq!(leave[0].pick, Pick::LeaveSite);
    }

    #[test]
    fn workspaces_commands_and_text() {
        let items = workspaces(vec!["default".into(), "lab".into()], "lab");
        assert_eq!(items[1].label, "lab  (this window)");
        assert_eq!(items[0].detail, "ctrl+alt+1");
        assert_eq!(commands(true)[3].label, "Follow agent: turn off");
        assert_eq!(commands(true)[1].pick, Pick::NewSession);
        assert_eq!(commands(true)[1].label, "New session");
        assert_eq!(commands(true)[2].pick, Pick::AttachFile);
        let groups: Vec<&str> = commands(false).iter().map(|i| i.group).collect();
        assert_eq!(
            groups,
            [
                "session",
                "session",
                "session",
                "session",
                "session",
                "session",
                "session",
                "stage",
                "workspace",
                "workspace",
                "workspace"
            ],
            "grouped as shown"
        );
        assert_eq!(short("abcdef", 3), "abc…");
        assert_eq!(short("abc", 3), "abc");
        assert_eq!(short("héllo wörld", 4), "héll…", "by characters");
        assert_eq!(project_name("/a/b/game/"), "game");
    }

    #[test]
    fn projects_and_models() {
        let project = |name: &str, used: &str| ProjectSummary {
            name: name.into(),
            path: format!("~/Projects/{name}"),
            modified: String::new(),
            folder: format!("/home/u/Projects/{name}"),
            sessions: 0,
            running: 0,
            waiting: 0,
            last_used: Some(used.into()),
            page: None,
        };
        let items = projects(
            &[project("old", "2026-01-01"), project("game", "2026-11-01")],
            Some("/home/u/Projects/old"),
        );
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["game", "old · shown", "No project"]);
        assert_eq!(items[2].pick, Pick::SetProject(None));
        let model = ModelInfo {
            id: "m1".into(),
            name: Some("Model One".into()),
            context: Some(128_000),
        };
        let items = models(&[model], "m0", Some("m1"));
        assert_eq!(items[0].label, "Server default");
        assert_eq!(items[0].detail, "m0");
        assert_eq!(items[1].label, "m1 · shown");
        assert_eq!(items[1].detail, "128k context");
    }

    #[test]
    fn a_new_session_starts_the_orchestrator_in_the_picked_project() {
        let project = |name: &str, used: &str| ProjectSummary {
            name: name.into(),
            path: format!("~/Projects/{name}"),
            modified: String::new(),
            folder: format!("/home/u/Projects/{name}"),
            sessions: 0,
            running: 0,
            waiting: 0,
            last_used: Some(used.into()),
            page: None,
        };
        let items =
            new_session_projects(&[project("old", "2026-01-01"), project("game", "2026-11-01")]);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            ["game", "old", "No project"],
            "recent first, then none"
        );
        assert_eq!(
            items[0].pick,
            Pick::NewSessionIn(Some("/home/u/Projects/game".into()))
        );
        assert_eq!(items[2].pick, Pick::NewSessionIn(None));
        assert_eq!(items[2].detail, "a session on the sites only");
        assert!(items.iter().all(|i| i.group == "projects"));
        match new_session(Some("/home/u/Projects/game".into())) {
            ClientCommand::SessionCreate { agent, project } => {
                assert_eq!(agent, "orchestrator");
                assert_eq!(project.as_deref(), Some("/home/u/Projects/game"));
            }
            other => panic!("not a session create: {other:?}"),
        }
        match new_session(None) {
            ClientCommand::SessionCreate { agent, project } => {
                assert_eq!(agent, "orchestrator");
                assert_eq!(project, None);
            }
            other => panic!("not a session create: {other:?}"),
        }
    }
}
