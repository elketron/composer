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
    pub fn of(message: &ServerMessage, shown: Option<&str>, workspace: &str) -> Option<Self> {
        match message {
            ServerMessage::TurnDone { session, .. } if shown == Some(session.as_str()) => {
                Some(Self {
                    summary: "The agent is done",
                    body: format!("{workspace} · #{session}"),
                })
            }
            ServerMessage::ApprovalRequest {
                session,
                title,
                site,
                recipe,
                ..
            } => {
                let what = if title.is_empty() {
                    format!("{site} › {recipe}")
                } else {
                    title.clone()
                };
                Some(Self {
                    summary: "An approval is waiting",
                    body: format!("{what} · {workspace} · #{session}"),
                })
            }
            _ => None,
        }
    }

    /// What `site.set` answered: the sentence it replied with, or why it failed.
    pub fn workflow(reply: Result<String, String>) -> Self {
        match reply {
            Ok(text) => Self {
                summary: "Workflow",
                body: text,
            },
            Err(why) => Self {
                summary: "Workflow not changed",
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
    fn a_workflow_switch_says_what_happened_or_why_not() {
        let said = "Entered 3d (3D). Started the blender application.";
        assert_eq!(Notice::workflow(Ok(said.into())).body, said);
        let refused = Notice::workflow(Err("no site `x`".into()));
        assert_eq!(refused.summary, "Workflow not changed");
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
            Notice::of(&done("a"), Some("a"), "lab"),
            Some(Notice {
                summary: "The agent is done",
                body: "lab · #a".into()
            })
        );
        assert_eq!(
            Notice::of(&done("b"), Some("a"), "lab"),
            None,
            "another session's turn"
        );
        let approval = message(serde_json::json!({
            "type": "approval.request", "request": "r1", "session": "b", "title": "",
            "site": "cad", "recipe": "export", "args": [], "body": "", "hash": "h"
        }));
        assert_eq!(
            Notice::of(&approval, Some("a"), "lab").map(|n| n.body),
            Some("cad › export · lab · #b".into())
        );
        let other = message(serde_json::json!({"type": "turn.started", "session": "a", "seq": 2}));
        assert_eq!(Notice::of(&other, Some("a"), "lab"), None);
    }
}
