use std::collections::BTreeMap;

use base64::Engine;
use composer_protocol::{ClientCommand, CommandResult, Desk, ServerMessage, SessionInfo};

use crate::{Approval, TerminalView};

/// What the server said, reduced to what a client shows (no UI): the sessions, the attached
/// session's desk, terminals and approvals.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Store {
    pub sessions: Vec<SessionInfo>,
    /// The session this client shows.
    pub attached: Option<String>,
    pub desk: Desk,
    /// The tab the agent opened or pointed at last (a following client shows it).
    pub pointed: Option<String>,
    pub terminals: BTreeMap<String, TerminalView>,
    /// Approvals asked and not yet answered, by request id.
    pub approvals: BTreeMap<String, Approval>,
}

impl Store {
    /// A new socket starts a fresh snapshot of open approvals. Resolutions missed while
    /// disconnected are not replayed; the server sends only the requests still waiting.
    pub fn reconnected(&mut self) {
        self.approvals.clear();
    }

    /// Applies a server message; what the client should send because of it.
    pub fn apply(&mut self, message: &ServerMessage) -> Vec<ClientCommand> {
        match message {
            ServerMessage::Ok { result, .. } => self.reply(result),
            ServerMessage::SessionUpdated { session } => {
                self.upsert(session.clone());
                vec![]
            }
            ServerMessage::DeskChanged {
                session,
                desk,
                opened,
                ..
            } if self.is_attached(session) => {
                self.desk = desk.clone();
                if opened.is_some() {
                    self.pointed = opened.clone();
                }
                vec![]
            }
            ServerMessage::TerminalOutput { terminal, data, .. } => {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                    self.terminals
                        .entry(terminal.clone())
                        .or_default()
                        .push(&bytes);
                }
                vec![]
            }
            ServerMessage::TerminalExited { terminal, .. } => {
                self.terminals.entry(terminal.clone()).or_default().exited = true;
                vec![]
            }
            ServerMessage::ApprovalRequest {
                request,
                session,
                kind,
                title,
                site,
                recipe,
                args,
                body,
                rule,
                ..
            } => {
                let approval = Approval {
                    request: request.clone(),
                    session: session.clone(),
                    kind: *kind,
                    title: title.clone(),
                    site: site.clone(),
                    recipe: recipe.clone(),
                    args: args.clone(),
                    body: body.clone(),
                    rule: rule.clone(),
                };
                self.approvals.insert(request.clone(), approval);
                vec![]
            }
            ServerMessage::ApprovalResolved { request, .. } => {
                self.approvals.remove(request);
                vec![]
            }
            _ => vec![],
        }
    }

    /// Shows `session` now: its desk is fetched again.
    pub fn attach(&mut self, session: &str) {
        if self.attached.as_deref() != Some(session) {
            self.attached = Some(session.to_string());
            self.desk = Desk::default();
            self.pointed = None;
        }
    }

    fn reply(&mut self, result: &CommandResult) -> Vec<ClientCommand> {
        match result {
            // The whole list, in the server's order (newest first).
            CommandResult::Sessions { sessions } => self.sessions = sessions.clone(),
            CommandResult::Session { session } => self.upsert(session.clone()),
            CommandResult::Desk { desk } => self.desk = desk.clone(),
            CommandResult::Terminal {
                terminal,
                scrollback,
            } => {
                let mut view = TerminalView::default();
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(scrollback) {
                    view.push(&bytes);
                }
                self.terminals.insert(terminal.clone(), view);
            }
            _ => {}
        }
        vec![]
    }

    fn upsert(&mut self, session: SessionInfo) {
        match self.sessions.iter_mut().find(|s| s.id == session.id) {
            Some(known) => *known = session,
            None => self.sessions.insert(0, session),
        }
    }

    fn is_attached(&self, session: &str) -> bool {
        self.attached.as_deref() == Some(session)
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use composer_protocol::{DeskBy, DeskEntry, DeskTab, SessionInfo, TurnState};

    use super::*;

    fn session(id: &str) -> SessionInfo {
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": "agentic", "parent": null,
            "state": "idle", "last_seq": 0, "preview": null, "updated": "2026-11-07T00:00:00Z"
        }))
        .unwrap()
    }

    fn desk(tab: &str) -> Desk {
        Desk {
            tabs: vec![DeskEntry {
                id: format!("terminal:{tab}"),
                tab: DeskTab::Terminal { id: tab.into() },
                unseen: true,
            }],
            active: None,
        }
    }

    fn b64(text: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(text)
    }

    #[test]
    fn reconnect_replaces_the_approval_snapshot() {
        let mut store = Store::default();
        let asked = |request: &str| {
            serde_json::from_value::<ServerMessage>(serde_json::json!({
                "type": "approval.request", "request": request, "session": "a",
                "site": "tools", "recipe": "greet", "args": [], "body": "", "hash": "h"
            }))
            .unwrap()
        };
        store.apply(&asked("answered-while-offline"));
        store.apply(&asked("still-open"));
        store.reconnected();
        assert!(store.approvals.is_empty());
        // Only requests still waiting arrive on the new connection.
        store.apply(&asked("still-open"));
        assert_eq!(
            store
                .approvals
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["still-open"]
        );
    }

    #[test]
    fn approvals_wait_until_answered() {
        let mut store = Store::default();
        let asked = |request: &str, session: &str| {
            serde_json::from_value::<ServerMessage>(serde_json::json!({
                "type": "approval.request", "request": request, "session": session,
                "site": "tools", "recipe": "greet", "args": [], "body": "", "hash": "h"
            }))
            .unwrap()
        };
        store.apply(&asked("r1", "a"));
        store.apply(&asked("r2", "b"));
        store.apply(&asked("r1", "a"));
        assert_eq!(
            store.approvals.len(),
            2,
            "asked again on reconnect: still one"
        );
        store.apply(&ServerMessage::ApprovalResolved {
            request: "r1".into(),
            session: "a".into(),
            approved: true,
        });
        let waiting: Vec<_> = store
            .approvals
            .values()
            .map(|a| a.session.as_str())
            .collect();
        assert_eq!(waiting, ["b"]);
        assert_eq!(store.approvals["r2"].recipe, "greet");
    }

    #[test]
    fn a_recorded_stream_reduces_to_what_a_client_shows() {
        let mut store = Store::default();
        store.apply(&ServerMessage::Ok {
            id: 1,
            result: CommandResult::Sessions {
                sessions: vec![session("a"), session("b")],
            },
        });
        store.apply(&ServerMessage::SessionUpdated {
            session: SessionInfo {
                state: TurnState::Running,
                ..session("a")
            },
        });
        assert_eq!(store.sessions.len(), 2);
        assert_eq!(store.sessions[0].state, TurnState::Running);

        // The desk of the attached session only.
        store.attach("a");
        let changed = |session: &str, tab: &str| ServerMessage::DeskChanged {
            session: session.into(),
            desk: desk(tab),
            by: DeskBy::Agent,
            opened: Some(format!("terminal:{tab}")),
        };
        store.apply(&changed("b", "x"));
        assert_eq!(store.desk, Desk::default(), "another session's desk");
        store.apply(&changed("a", "t1"));
        assert_eq!(store.desk, desk("t1"));
        assert_eq!(store.pointed.as_deref(), Some("terminal:t1"));
        // Terminals: scrollback, then output, then the end.
        store.apply(&ServerMessage::Ok {
            id: 2,
            result: CommandResult::Terminal {
                terminal: "t1".into(),
                scrollback: b64("$ "),
            },
        });
        store.apply(&ServerMessage::TerminalOutput {
            session: "a".into(),
            terminal: "t1".into(),
            data: b64("ls\r\n"),
        });
        assert_eq!(store.terminals["t1"].scrollback, b"$ ls\r\n");
        store.apply(&ServerMessage::TerminalExited {
            session: "a".into(),
            terminal: "t1".into(),
        });
        assert!(store.terminals["t1"].exited);
    }
}
