use composer_protocol::{ClientCommand, CommandResult, ServerMessage};

use crate::action::Action;
use crate::effect::Effect;
use crate::store::types::{
    ConnState, FlashLevel, Mode, Outgoing, Pane, Pending, PickerTarget, Startup,
};
use crate::store::{ConnectionStore, PageStore, PickerStore, SessionStore, UiStore};

/// All shared state. `reduce` is the only way it changes; it does no IO.
#[derive(Debug)]
pub struct AppState {
    pub conn: ConnectionStore,
    pub session: SessionStore,
    pub page: PageStore,
    pub ui: UiStore,
    pub picker: PickerStore,
    startup: Option<Startup>,
    default_agent: String,
}

impl AppState {
    pub fn new(startup: Startup, context_limit: u32) -> Self {
        Self {
            conn: ConnectionStore::new(context_limit),
            session: SessionStore::default(),
            page: PageStore::new(),
            ui: UiStore::new(),
            picker: PickerStore::new(),
            default_agent: startup.agent.clone(),
            startup: Some(startup),
        }
    }

    pub fn reduce(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Quit => {
                self.ui.quit = true;
                vec![Effect::Quit]
            }
            Action::SetMode(mode) => {
                self.ui.set_mode(mode);
                vec![]
            }
            Action::FocusPane(pane) => {
                self.ui.focus = pane;
                if self.ui.maximized.is_some() {
                    self.ui.maximized = Some(pane);
                }
                vec![]
            }
            Action::FocusNext => {
                self.ui.focus_next();
                vec![]
            }
            Action::ToggleMaximize => {
                self.ui.toggle_maximize();
                vec![]
            }
            Action::ToggleThinking => {
                self.ui.show_thinking = !self.ui.show_thinking;
                vec![]
            }
            Action::ToggleToolDetails => {
                self.ui.show_tool_details = !self.ui.show_tool_details;
                vec![]
            }
            Action::Flash(text, level) => {
                self.ui.flash(text, level);
                vec![]
            }

            Action::SendMessage(text) => {
                let Some(session) = self.session.current.clone() else {
                    self.ui.flash("no session yet", FlashLevel::Warn);
                    return vec![];
                };
                self.ui.flash = None;
                self.send(
                    ClientCommand::MessageSend {
                        session,
                        text,
                        pages: vec![],
                        images: vec![],
                    },
                    Pending::Ignore,
                )
            }
            Action::CancelTurn => match self.session.current.clone() {
                Some(session) if self.session.is_busy() => {
                    self.send(ClientCommand::TurnCancel { session }, Pending::Ignore)
                }
                _ => vec![],
            },
            Action::NewSession { agent } => {
                let agent = agent.unwrap_or_else(|| self.default_agent.clone());
                let project = self.session.current_info().and_then(|s| s.project.clone());
                self.send(
                    ClientCommand::SessionCreate { agent, project },
                    Pending::CreateSession,
                )
            }
            Action::AttachSession(id) => self.attach(&id, true),
            Action::JumpBack => match self.session.jumplist.pop() {
                Some(id) => self.attach(&id, false),
                None => {
                    self.ui.flash("nothing to jump back to", FlashLevel::Info);
                    vec![]
                }
            },
            Action::SetProject(path) => match self.session.current.clone() {
                Some(session) => {
                    self.send(ClientCommand::ProjectSet { session, path }, Pending::Ignore)
                }
                None => vec![],
            },

            Action::OpenUrl(url) => {
                if !url.starts_with("site://") {
                    self.ui
                        .flash(format!("external link: {url}"), FlashLevel::Info);
                    return vec![];
                }
                // You navigated yourself: stop following the agent and look at the page.
                self.page.follow = false;
                self.reduce(Action::FocusPane(Pane::Page));
                vec![self.page.navigate(url)]
            }
            Action::PageBack => self.page.back().into_iter().collect(),
            Action::PageForward => self.page.forward().into_iter().collect(),
            Action::ReloadPage => self.page.reload().into_iter().collect(),
            Action::ToggleFollow => {
                self.page.follow = !self.page.follow;
                let text = if self.page.follow {
                    "following the agent"
                } else {
                    "not following the agent"
                };
                self.ui.flash(text, FlashLevel::Info);
                vec![]
            }
            Action::EditPage => match &self.page.current {
                Some(page) => vec![Effect::EditPage(page.url.clone())],
                None => vec![],
            },

            Action::OpenPicker(kind) => {
                self.ui.picker_open = true;
                self.picker.open(kind, &self.session.sessions)
            }
            Action::PickerQuery(query) => self.picker.set_query(query),
            Action::PickerMove(delta) => {
                self.picker.move_selection(delta);
                vec![]
            }
            Action::PickerAccept => {
                self.ui.picker_open = false;
                match self.picker.selected_target() {
                    Some(PickerTarget::Url(url)) => self.reduce(Action::OpenUrl(url)),
                    Some(PickerTarget::Session(id)) => self.attach(&id, true),
                    None => vec![],
                }
            }
            Action::ClosePicker => {
                self.ui.picker_open = false;
                vec![]
            }

            Action::Connected => self.connected(),
            Action::Disconnected(reason) => {
                self.conn.state = ConnState::Disconnected(reason);
                vec![]
            }
            Action::Server(message) => self.server_message(message),
            Action::PageLoaded(page) => {
                self.page.loaded(page);
                vec![]
            }
            Action::PageFailed { url, error } => {
                self.page.failed(&url, error.clone());
                self.ui.flash(error, FlashLevel::Error);
                vec![]
            }
            Action::SearchResults { query, hits } => {
                self.picker.search_results(&query, hits);
                vec![]
            }
            Action::SitesLoaded(sites) => {
                self.picker.sites(sites);
                vec![]
            }
        }
    }

    fn send(&mut self, command: ClientCommand, pending: Pending) -> Vec<Effect> {
        if !self.conn.is_connected() {
            self.ui
                .flash("not connected to the server", FlashLevel::Error);
            return vec![];
        }
        vec![self.conn.request(Outgoing { command, pending })]
    }

    fn attach(&mut self, id: &str, remember: bool) -> Vec<Effect> {
        self.session.begin_attach(id, remember);
        self.send(
            ClientCommand::SessionAttach {
                session: id.to_string(),
                since: None,
            },
            Pending::Attach(id.to_string()),
        )
    }

    fn connected(&mut self) -> Vec<Effect> {
        self.conn.state = ConnState::Connected;
        let mut effects = self.send(
            ClientCommand::Subscribe {
                urls: vec!["*".into()],
            },
            Pending::Ignore,
        );
        effects.extend(self.send(ClientCommand::SessionList, Pending::ListSessions));
        // After a reconnect, catch up on what we missed.
        if let Some(current) = self.session.current.clone() {
            let since = Some(self.session.last_seq);
            effects.extend(self.send(
                ClientCommand::SessionAttach {
                    session: current.clone(),
                    since,
                },
                Pending::Attach(current),
            ));
        }
        effects
    }

    fn server_message(&mut self, message: ServerMessage) -> Vec<Effect> {
        match message {
            ServerMessage::Ok { id, result } => self.reply(id, result),
            ServerMessage::Error { id, message } => {
                if let Some(id) = id {
                    self.conn.take_pending(id);
                }
                self.ui.flash(message, FlashLevel::Error);
                vec![]
            }
            ServerMessage::SessionUpdated { session } => {
                self.session.upsert(session);
                vec![]
            }
            ServerMessage::Slots {
                used,
                total,
                waiting,
            } => {
                (
                    self.conn.slots_used,
                    self.conn.slots_total,
                    self.conn.slots_waiting,
                ) = (used, total, waiting);
                vec![]
            }
            ServerMessage::PageChanged { url, .. } if self.page.shows(&url) => {
                self.page.reload().into_iter().collect()
            }
            ServerMessage::PageChanged { .. } | ServerMessage::WorkerStatus { .. } => vec![],
            ServerMessage::ApprovalRequest { recipe, .. } => {
                self.ui.flash(
                    format!("recipe `{recipe}` needs approval (milestone 4)"),
                    FlashLevel::Warn,
                );
                vec![]
            }
            ServerMessage::AgentNavigated { ref url, .. } => {
                let url = url.clone();
                let current = self.session.apply(&message);
                if current && self.page.follow {
                    vec![self.page.navigate(url)]
                } else {
                    vec![]
                }
            }
            other => {
                self.session.apply(&other);
                vec![]
            }
        }
    }

    fn reply(&mut self, id: u64, result: CommandResult) -> Vec<Effect> {
        match (self.conn.take_pending(id), result) {
            (Pending::ListSessions, CommandResult::Sessions { sessions }) => {
                for info in sessions.into_iter().rev() {
                    self.session.upsert(info);
                }
                self.run_startup()
            }
            (Pending::CreateSession, CommandResult::Session { session }) => {
                let id = session.id.clone();
                self.session.upsert(session);
                self.attach(&id, true)
            }
            (Pending::Attach(_), CommandResult::Session { session }) => {
                self.session.upsert(session);
                vec![]
            }
            _ => vec![],
        }
    }

    fn run_startup(&mut self) -> Vec<Effect> {
        let Some(startup) = self.startup.take() else {
            return vec![];
        };
        if let Some(id) = startup.session {
            return self.attach(&id, false);
        }
        let latest = self
            .session
            .sessions
            .iter()
            .find(|s| s.parent.is_none())
            .map(|s| s.id.clone());
        match latest {
            Some(id) if !startup.new => self.attach(&id, false),
            _ => self.send(
                ClientCommand::SessionCreate {
                    agent: startup.agent,
                    project: startup.project,
                },
                Pending::CreateSession,
            ),
        }
    }

    /// Mode helper for components.
    pub fn mode(&self) -> Mode {
        self.ui.mode
    }
}

#[cfg(test)]
mod tests {
    use composer_protocol::{ChatMessage, Role, SessionInfo, TurnState};

    use super::*;
    use crate::store::types::{ChatItem, PickerKind};

    fn info(id: &str, parent: Option<&str>) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            agent: "default".into(),
            project: None,
            ticket: None,
            parent: parent.map(Into::into),
            state: TurnState::Idle,
            last_seq: 0,
            context_tokens: 0,
            preview: None,
            updated: String::new(),
        }
    }

    fn connected() -> (AppState, Vec<Effect>) {
        let mut state = AppState::new(
            Startup {
                agent: "default".into(),
                ..Startup::default()
            },
            160_000,
        );
        let effects = state.reduce(Action::Connected);
        (state, effects)
    }

    fn sent(effects: &[Effect]) -> Vec<(u64, ClientCommand)> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Send(r) => Some((r.id, r.command.clone())),
                _ => None,
            })
            .collect()
    }

    fn assistant(content: &str) -> ChatMessage {
        ChatMessage {
            role: Role::Assistant,
            content: content.into(),
            thinking: Some("hmm".into()),
            tool_calls: vec![],
            pages: vec![],
            images: vec![],
            source: None,
        }
    }

    #[test]
    fn startup_resumes_latest_top_level_session() {
        let (mut state, effects) = connected();
        let list_id = sent(&effects)
            .iter()
            .find(|(_, c)| *c == ClientCommand::SessionList)
            .unwrap()
            .0;
        let sessions = vec![info("w1", Some("o1")), info("o1", None)];
        let effects = state.reduce(Action::Server(ServerMessage::Ok {
            id: list_id,
            result: CommandResult::Sessions { sessions },
        }));
        assert_eq!(
            sent(&effects)[0].1,
            ClientCommand::SessionAttach {
                session: "o1".into(),
                since: None
            }
        );
        assert_eq!(state.session.current.as_deref(), Some("o1"));
    }

    #[test]
    fn startup_creates_session_when_none_exist() {
        let (mut state, effects) = connected();
        let list_id = sent(&effects)
            .iter()
            .find(|(_, c)| *c == ClientCommand::SessionList)
            .unwrap()
            .0;
        let effects = state.reduce(Action::Server(ServerMessage::Ok {
            id: list_id,
            result: CommandResult::Sessions { sessions: vec![] },
        }));
        assert!(matches!(
            sent(&effects)[0].1,
            ClientCommand::SessionCreate { .. }
        ));
    }

    #[test]
    fn streaming_then_done_and_stale_events_ignored() {
        let (mut state, _) = connected();
        state.session.upsert(info("s", None));
        state.reduce(Action::AttachSession("s".into()));
        let delta = |seq, text: &str| ServerMessage::MessageDelta {
            session: "s".into(),
            seq,
            text: text.into(),
        };
        state.reduce(Action::Server(delta(1, "Hel")));
        state.reduce(Action::Server(delta(2, "lo")));
        assert_eq!(state.session.streaming.as_ref().unwrap().content, "Hello");
        state.reduce(Action::Server(ServerMessage::MessageDone {
            session: "s".into(),
            seq: 3,
            message: assistant("Hello"),
        }));
        assert!(state.session.streaming.is_none());
        assert_eq!(
            state.session.items,
            [
                ChatItem::Thinking { text: "hmm".into() },
                ChatItem::Text {
                    text: "Hello".into()
                }
            ]
        );
        // A replayed (older) event is ignored; another session's events too.
        state.reduce(Action::Server(delta(2, "dup")));
        state.reduce(Action::Server(ServerMessage::MessageDelta {
            session: "other".into(),
            seq: 9,
            text: "x".into(),
        }));
        assert!(state.session.streaming.is_none());
    }

    #[test]
    fn agent_navigation_follows_until_you_navigate() {
        let (mut state, _) = connected();
        state.session.upsert(info("s", None));
        state.reduce(Action::AttachSession("s".into()));
        let nav = |seq, url: &str| {
            Action::Server(ServerMessage::AgentNavigated {
                session: "s".into(),
                seq,
                url: url.into(),
            })
        };
        assert_eq!(
            state.reduce(nav(1, "site://memory/index.md")),
            [Effect::FetchPage("site://memory/index.md".into())]
        );
        state.reduce(Action::OpenUrl("site://plans".into()));
        assert_eq!(state.ui.focus, Pane::Page);
        assert!(!state.page.follow);
        assert!(state.reduce(nav(2, "site://home/index.md")).is_empty());
    }

    #[test]
    fn picker_accepting_a_session_attaches_and_remembers_the_jump() {
        let (mut state, _) = connected();
        state.session.upsert(info("a", None));
        state.session.upsert(info("b", None));
        state.reduce(Action::AttachSession("a".into()));
        state.reduce(Action::OpenPicker(PickerKind::Sessions));
        state.reduce(Action::PickerQuery("#b".into()));
        let effects = state.reduce(Action::PickerAccept);
        assert_eq!(
            sent(&effects)[0].1,
            ClientCommand::SessionAttach {
                session: "b".into(),
                since: None
            }
        );
        assert!(!state.ui.picker_open);
        let effects = state.reduce(Action::JumpBack);
        assert_eq!(
            sent(&effects)[0].1,
            ClientCommand::SessionAttach {
                session: "a".into(),
                since: None
            }
        );
    }

    #[test]
    fn nothing_is_sent_while_disconnected() {
        let (mut state, _) = connected();
        state.reduce(Action::Disconnected("gone".into()));
        state.session.current = Some("s".into());
        assert!(state.reduce(Action::SendMessage("hi".into())).is_empty());
        assert!(state.ui.flash.is_some());
    }
}
