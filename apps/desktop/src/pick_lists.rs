//! What the pickers list: workspaces, sessions (most recent first), the palette's commands.

use composer_protocol::SessionInfo;
use composer_protocol::http::SiteSummary;

use crate::picker::{Item, Pick};

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
            pick: Pick::Workspace(name),
        })
        .collect()
}

/// The user's sessions (not workers'), most recent first: what each is about, project, state.
pub fn sessions(sessions: &[SessionInfo], shown: Option<&str>) -> Vec<Item> {
    let mut sessions: Vec<&SessionInfo> = sessions.iter().filter(|s| s.worker.is_none()).collect();
    sessions.sort_by(|a, b| b.updated.cmp(&a.updated));
    sessions
        .into_iter()
        .map(|s| {
            let about = s.preview.clone().unwrap_or_else(|| s.agent.clone());
            let mark = if shown == Some(s.id.as_str()) {
                "  ◂ shown"
            } else {
                ""
            };
            Item {
                label: format!("#{} {}{mark}", s.id, short(&about, 44)),
                detail: format!(
                    "{} · {:?}",
                    s.project
                        .as_deref()
                        .map(project_name)
                        .unwrap_or("no project"),
                    s.state
                )
                .to_lowercase(),
                pick: Pick::Session(s.id.clone()),
            }
        })
        .collect()
}

pub fn commands(following: bool) -> Vec<Item> {
    let follow = if following { "off" } else { "on" };
    vec![
        Item {
            label: "New terminal".into(),
            detail: "+".into(),
            pick: Pick::NewTerminal,
        },
        Item {
            label: "Attach a file…".into(),
            detail: "or drop it on the chat".into(),
            pick: Pick::AttachFile,
        },
        Item {
            label: format!("Follow agent: turn {follow}"),
            detail: String::new(),
            pick: Pick::ToggleFollow,
        },
        Item {
            label: "Switch workspace…".into(),
            detail: "ctrl+shift+w".into(),
            pick: Pick::Workspaces,
        },
        Item {
            label: "Switch session…".into(),
            detail: "ctrl+shift+s".into(),
            pick: Pick::Sessions,
        },
    ]
}

/// "leave workflow" while the shown session is in one.
pub fn leave_workflow(current: Option<&str>) -> Vec<Item> {
    current
        .map(|site| Item {
            label: "leave workflow".into(),
            detail: site.to_string(),
            pick: Pick::LeaveWorkflow,
        })
        .into_iter()
        .collect()
}

/// One "workflow: <name>" per workflow site (one with an `app:` or an `mcp:`), the one the
/// session is in marked.
pub fn workflows(sites: &[SiteSummary], current: Option<&str>) -> Vec<Item> {
    sites
        .iter()
        .filter(|site| site.app.is_some() || !site.mcp.is_empty())
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
                label: format!("workflow: {}", site.name),
                detail: about.join(" · "),
                pick: Pick::Workflow(site.name.clone()),
            }
        })
        .collect()
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
            "id": id, "agent": "default", "project": "/home/u/Projects/game/", "ticket": null,
            "parent": null, "state": "idle", "last_seq": 0, "preview": preview, "updated": updated
        });
        if worker {
            json["worker"] = serde_json::json!("running");
        }
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn sessions_recent_first_without_workers() {
        let all = [
            session("a", "2026-11-01T00:00:00Z", Some("old one"), false),
            session("b", "2026-11-07T00:00:00Z", None, false),
            session("w", "2026-11-08T00:00:00Z", None, true),
        ];
        let items = sessions(&all, Some("a"));
        let picks: Vec<&Pick> = items.iter().map(|i| &i.pick).collect();
        assert_eq!(
            picks,
            [&Pick::Session("b".into()), &Pick::Session("a".into())]
        );
        assert_eq!(items[0].label, "#b default", "no preview: the agent");
        assert_eq!(items[1].label, "#a old one  ◂ shown");
        assert_eq!(items[1].detail, "game · idle");
    }

    fn site(name: &str, app: Option<&str>, mcp: &[&str]) -> SiteSummary {
        serde_json::from_value(serde_json::json!({
            "name": name, "title": name, "has_justfile": false, "has_ui": false,
            "app": app, "mcp": mcp
        }))
        .unwrap()
    }

    #[test]
    fn a_palette_entry_per_workflow_site_and_a_way_out() {
        let sites = [
            site("3d", Some("blender"), &["blender"]),
            site("notes", None, &[]),
            site("research", None, &["wayai", "web"]),
        ];
        let items = workflows(&sites, Some("3d"));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["workflow: 3d", "workflow: research"]);
        assert_eq!(items[0].pick, Pick::Workflow("3d".into()));
        assert_eq!(items[0].detail, "app blender · mcp blender · ◂ here");
        assert_eq!(items[1].detail, "mcp wayai, web");
        assert!(leave_workflow(None).is_empty());
        let leave = leave_workflow(Some("3d"));
        assert_eq!(leave[0].label, "leave workflow");
        assert_eq!(leave[0].pick, Pick::LeaveWorkflow);
    }

    #[test]
    fn workspaces_commands_and_text() {
        let items = workspaces(vec!["default".into(), "lab".into()], "lab");
        assert_eq!(items[1].label, "lab  (this window)");
        assert_eq!(items[0].detail, "ctrl+alt+1");
        assert_eq!(commands(true)[2].label, "Follow agent: turn off");
        assert_eq!(commands(true)[1].pick, Pick::AttachFile);
        assert_eq!(short("abcdef", 3), "abc…");
        assert_eq!(short("abc", 3), "abc");
        assert_eq!(short("héllo wörld", 4), "héll…", "by characters");
        assert_eq!(project_name("/a/b/game/"), "game");
    }
}
