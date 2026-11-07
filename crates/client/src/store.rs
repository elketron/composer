use std::collections::BTreeMap;

use base64::Engine;
use composer_protocol::http::OpenProject;
use composer_protocol::{
    ClientCommand, CommandResult, Desk, Selection, ServerMessage, SessionInfo,
};

use crate::{TerminalView, WindowBinding};

/// What the server said, reduced to what a client shows (no UI): the sessions, the attached
/// session's desk and selection, terminals, and the window this client is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Store {
    pub sessions: Vec<SessionInfo>,
    /// The session this client shows.
    pub attached: Option<String>,
    pub desk: Desk,
    pub selection: Option<Selection>,
    /// The tab the agent opened or pointed at last (a following client shows it).
    pub pointed: Option<String>,
    pub terminals: BTreeMap<String, TerminalView>,
    /// The projects some client has open (`projects.open`; no event: ask again to refresh).
    pub open_projects: Vec<OpenProject>,
    /// The project this client opened (`project.open`).
    pub opened_project: Option<String>,
    /// This client's desktop window, and its TUI's terminal.
    pub window: Option<WindowBinding>,
}

impl Store {
    /// Applies a server message; what the client should send because of it (when the window's
    /// TUI shows another session: attach to it and get its desk).
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
            ServerMessage::SelectionChanged { session, selection } if self.is_attached(session) => {
                self.selection = selection.clone();
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
            ServerMessage::WindowChanged { window, session }
                if self.window.as_ref().is_some_and(|w| w.is(window)) =>
            {
                self.follow(session)
            }
            _ => vec![],
        }
    }

    /// Shows `session` now: its desk and selection are fetched again.
    pub fn attach(&mut self, session: &str) {
        if self.attached.as_deref() != Some(session) {
            self.attached = Some(session.to_string());
            self.desk = Desk::default();
            self.selection = None;
            self.pointed = None;
        }
    }

    fn follow(&mut self, session: &str) -> Vec<ClientCommand> {
        if self.attached.as_deref() == Some(session) {
            return vec![];
        }
        self.attach(session);
        vec![
            ClientCommand::SessionAttach {
                session: session.to_string(),
                since: None,
            },
            ClientCommand::DeskGet {
                session: session.to_string(),
            },
        ]
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
            CommandResult::OpenProjects { projects } => self.open_projects = projects.clone(),
            CommandResult::ProjectOpened { project, .. } => {
                self.opened_project = Some(project.clone());
            }
            CommandResult::Window { window, terminal } => {
                self.window = Some(WindowBinding {
                    window: window.clone(),
                    terminal: terminal.clone(),
                });
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
            "id": id, "agent": "default", "project": null, "ticket": null, "parent": null,
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

        // The desk and selection of the attached session only.
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
        let selection = Selection {
            app: "board".into(),
            label: "ticket 7".into(),
            text: None,
            url: None,
            data: None,
        };
        store.apply(&ServerMessage::SelectionChanged {
            session: "b".into(),
            selection: Some(selection.clone()),
        });
        assert_eq!(store.selection, None, "another session's selection");
        store.apply(&ServerMessage::SelectionChanged {
            session: "a".into(),
            selection: Some(selection.clone()),
        });
        assert_eq!(store.selection, Some(selection));

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
            window: None,
        });
        assert_eq!(store.terminals["t1"].scrollback, b"$ ls\r\n");
        store.apply(&ServerMessage::TerminalExited {
            session: "a".into(),
            terminal: "t1".into(),
            window: None,
        });
        assert!(store.terminals["t1"].exited);

        // Open projects: the list, and the one this client opened.
        store.apply(&ServerMessage::Ok {
            id: 4,
            result: CommandResult::OpenProjects {
                projects: vec![OpenProject {
                    name: "composer".into(),
                    clients: 2,
                }],
            },
        });
        store.apply(&ServerMessage::Ok {
            id: 5,
            result: CommandResult::ProjectOpened {
                project: "composer".into(),
                first: false,
            },
        });
        assert_eq!(store.open_projects[0].name, "composer");
        assert_eq!(store.opened_project.as_deref(), Some("composer"));

        // The window: its TUI shows b, so the store attaches to b and asks for its desk.
        store.apply(&ServerMessage::Ok {
            id: 3,
            result: CommandResult::Window {
                window: "w1".into(),
                terminal: "t9".into(),
            },
        });
        assert!(
            store
                .apply(&ServerMessage::WindowChanged {
                    window: "w2".into(),
                    session: "b".into()
                })
                .is_empty(),
            "another window"
        );
        let follow = store.apply(&ServerMessage::WindowChanged {
            window: "w1".into(),
            session: "b".into(),
        });
        assert_eq!(store.attached.as_deref(), Some("b"));
        assert!(store.desk.tabs.is_empty() && store.selection.is_none());
        assert_eq!(
            follow,
            [
                ClientCommand::SessionAttach {
                    session: "b".into(),
                    since: None
                },
                ClientCommand::DeskGet {
                    session: "b".into()
                }
            ]
        );
    }
}
