//! The chat as data, with no gpui in it: the session's transcript ([`Chat`]) and its turn state.
//! [`crate::chat_pane::ChatPane`] draws it and hands it every event; what to send is decided here.

use composer_client::Chat;
use composer_protocol::{ServerMessage, TurnState};

use crate::outgoing::Outgoing;

#[derive(Debug)]
pub struct ChatModel {
    chat: Option<Chat>,
    state: TurnState,
}

impl Default for ChatModel {
    fn default() -> Self {
        Self {
            chat: None,
            state: TurnState::Idle,
        }
    }
}

impl ChatModel {
    pub fn chat(&self) -> Option<&Chat> {
        self.chat.as_ref()
    }

    pub fn session(&self) -> Option<&str> {
        self.chat.as_ref().map(|c| c.session.as_str())
    }

    pub fn state(&self) -> TurnState {
        self.state
    }

    /// Show `session`; its chat starts empty until its history arrives. `false` when it is the
    /// one shown already.
    pub fn open(&mut self, session: &str) -> bool {
        if self.session() == Some(session) {
            return false;
        }
        self.chat = Some(Chat::new(session));
        true
    }

    /// The session's turn state. `true` when it changed.
    pub fn set_state(&mut self, state: TurnState) -> bool {
        let changed = self.state != state;
        self.state = state;
        changed
    }

    /// The session's last messages.
    pub fn history_loaded(&mut self, session: &str, events: &[ServerMessage], more: bool) -> bool {
        let Some(chat) = self.chat.as_mut().filter(|c| c.session == session) else {
            return false;
        };
        chat.history_loaded(events, more);
        true
    }

    /// Older messages, put in front: how many items they added, `None` for another session.
    pub fn older_loaded(
        &mut self,
        session: &str,
        events: &[ServerMessage],
        more: bool,
    ) -> Option<usize> {
        let chat = self.chat.as_mut().filter(|c| c.session == session)?;
        let before = chat.items.len();
        chat.older_loaded(events, more);
        Some(chat.items.len() - before)
    }

    /// A transcript event. `true` when it changed the chat.
    pub fn apply(&mut self, message: &ServerMessage) -> bool {
        let Some(chat) = self.chat.as_mut() else {
            return false;
        };
        chat.apply(message)
    }

    /// A message from the composer: always sent.
    pub fn send(&self, text: String) -> Option<Outgoing> {
        let chat = self.chat.as_ref()?;
        Some(Outgoing {
            session: chat.session.clone(),
            text,
        })
    }
}
