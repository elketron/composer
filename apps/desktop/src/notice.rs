//! What a message is worth telling the user about while the window is behind.

use composer_protocol::ServerMessage;

/// A notification's summary and body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub summary: &'static str,
    pub body: String,
}

impl Notice {
    /// A finished turn of the shown session, or any approval waiting; nothing else.
    pub fn of(message: &ServerMessage, shown: Option<&str>) -> Option<Self> {
        match message {
            ServerMessage::TurnDone { session, .. } if shown == Some(session.as_str()) => {
                Some(Self {
                    summary: "The agent is done",
                    body: format!("#{session}"),
                })
            }
            ServerMessage::ApprovalRequest {
                session,
                title,
                site,
                recipe,
                args,
                ..
            } => {
                let what = if recipe == crate::home_lists::MCP_FILE {
                    format!("Start {site}'s MCP servers")
                } else if recipe == crate::home_lists::FOLDER {
                    crate::home_lists::folder_use(site, args)
                } else if recipe == crate::home_lists::POLICY {
                    crate::home_lists::policy_use(site)
                } else if title.is_empty() {
                    format!("{site} › {recipe}")
                } else {
                    title.clone()
                };
                Some(Self {
                    summary: "An approval is waiting",
                    body: format!("{what} · #{session}"),
                })
            }
            _ => None,
        }
    }

    /// What `site.set` answered: the sentence it replied with, or why it failed.
    pub fn site(reply: Result<String, String>) -> Self {
        match reply {
            Ok(text) => Self {
                summary: "Site",
                body: text,
            },
            Err(why) => Self {
                summary: "Site not changed",
                body: why,
            },
        }
    }

    /// Shown by the desktop's notification service (off the window's thread: it may block).
    pub fn show(self) {
        std::thread::spawn(move || {
            let _ = notify_rust::Notification::new()
                .appname("composer")
                .summary(self.summary)
                .body(&self.body)
                .show();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(json: serde_json::Value) -> ServerMessage {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn a_site_switch_says_what_happened_or_why_not() {
        let said = "Entered 3d (3D).";
        assert_eq!(Notice::site(Ok(said.into())).body, said);
        let refused = Notice::site(Err("no site `x`".into()));
        assert_eq!(refused.summary, "Site not changed");
        assert_eq!(refused.body, "no site `x`");
    }

    #[test]
    fn turns_of_the_shown_session_and_approvals() {
        let done = |s: &str| {
            message(
                serde_json::json!({"type": "turn.done", "session": s, "seq": 1, "cancelled": false}),
            )
        };
        assert_eq!(
            Notice::of(&done("a"), Some("a")),
            Some(Notice {
                summary: "The agent is done",
                body: "#a".into()
            })
        );
        assert_eq!(
            Notice::of(&done("b"), Some("a")),
            None,
            "another session's turn"
        );
        let approval = message(serde_json::json!({
            "type": "approval.request", "request": "r1", "session": "b", "title": "",
            "site": "cad", "recipe": "export", "args": [], "body": "", "hash": "h"
        }));
        assert_eq!(
            Notice::of(&approval, Some("a")).map(|n| n.body),
            Some("cad › export · #b".into())
        );
        let folder = message(serde_json::json!({
            "type": "approval.request", "request": "r2", "session": "b", "title": "",
            "site": "game", "recipe": "folder:", "args": ["/home/me/game"], "body": "", "hash": "h"
        }));
        assert_eq!(
            Notice::of(&folder, Some("a")).map(|n| n.body),
            Some("Let game work in /home/me/game · #b".into())
        );
        let policy = message(serde_json::json!({
            "type": "approval.request", "request": "r3", "session": "b", "title": "",
            "site": "game", "recipe": "approvals.md", "args": [], "body": "", "hash": "h"
        }));
        assert_eq!(
            Notice::of(&policy, Some("a")).map(|n| n.body),
            Some("let system-1 follow game's approvals.md · #b".into())
        );
        let other = message(serde_json::json!({"type": "turn.started", "session": "a", "seq": 2}));
        assert_eq!(Notice::of(&other, Some("a")), None);
    }
}
