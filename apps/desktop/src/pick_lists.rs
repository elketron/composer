//! What the pickers list: sessions (most recent first), the palette's commands, models.

use std::collections::BTreeMap;

use composer_client::Approval;
use composer_protocol::http::SiteSummary;
use composer_protocol::{ClientCommand, ModelInfo, SessionInfo};

use crate::picker::{Item, Pick};

// rewrite: phase 2: the run-kind choice replaces this.
/// The agent every session the desktop starts runs as: the server's built-in `default` (a page in
/// `site://agents` may override it). The desktop never asks for an agent.
pub const DEFAULT_AGENT: &str = "default";

/// The command that starts a session in `project` (a folder) or none.
pub fn new_session(project: Option<String>) -> ClientCommand {
    ClientCommand::SessionCreate {
        agent: DEFAULT_AGENT.into(),
        project,
    }
}

/// The sessions, most recent first: the agent and id, whether it is shown or waits for an
/// approval, its site and project, and what it last said underneath.
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
            detail: "in this session's project".into(),
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

    fn session(id: &str, updated: &str, preview: Option<&str>) -> SessionInfo {
        serde_json::from_value(serde_json::json!({
            "id": id, "agent": "default", "project": "/home/u/Projects/game/",
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
        assert_eq!(items[0].label, "default #b ! waiting");
        assert_eq!(items[0].note, "", "no preview: nothing under it");
        assert_eq!(items[1].label, "default #a · shown");
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
                "session", "session", "session", "session", "session", "session", "stage", "home"
            ],
            "grouped as shown"
        );
        assert_eq!(short("abcdef", 3), "abc…");
        assert_eq!(short("abc", 3), "abc");
        assert_eq!(short("héllo wörld", 4), "héll…", "by characters");
        assert_eq!(project_name("/a/b/game/"), "game");
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
    fn a_new_session_starts_the_default_agent() {
        for project in [Some("/home/u/Projects/game".to_string()), None] {
            match new_session(project.clone()) {
                ClientCommand::SessionCreate { agent, project: p } => {
                    assert_eq!(agent, "default");
                    assert_eq!(p, project);
                }
                other => panic!("not a session create: {other:?}"),
            }
        }
    }
}
