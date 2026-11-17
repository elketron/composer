//! The chat as data, with no gpui in it: the session's transcript ([`Chat`]), its turn state,
//! and the outbox of verdicts sent and not back yet. [`crate::chat_pane::ChatPane`] draws it and
//! hands it every event; what to send, and what an event does to the outbox, is decided here.

use composer_client::Chat;
use composer_protocol::{ServerMessage, TurnState};

use crate::outbox::Outbox;
use crate::outgoing::Outgoing;
use crate::user_review::{VerdictCard, on_pass, on_send, sync_outbox, verdict_card};

#[derive(Debug)]
pub struct ChatModel {
    chat: Option<Chat>,
    state: TurnState,
    outbox: Outbox,
}

impl Default for ChatModel {
    fn default() -> Self {
        Self {
            chat: None,
            state: TurnState::Idle,
            outbox: Outbox::default(),
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
    /// one shown already. The outbox keeps every session's entries.
    pub fn open(&mut self, session: &str) -> bool {
        if self.session() == Some(session) {
            return false;
        }
        self.chat = Some(Chat::new(session));
        true
    }

    /// The session's turn state. An `Idle` drops what it still owed from before a reconnect.
    /// `true` when something shown changed.
    pub fn set_state(&mut self, state: TurnState) -> bool {
        let drained = state == TurnState::Idle
            && self
                .chat
                .as_ref()
                .is_some_and(|chat| self.outbox.idle(&chat.session));
        let changed = self.state != state;
        self.state = state;
        drained || changed
    }

    /// The session's last messages; the outbox follows what they say.
    pub fn history_loaded(&mut self, session: &str, events: &[ServerMessage], more: bool) -> bool {
        let Some(chat) = self.chat.as_mut().filter(|c| c.session == session) else {
            return false;
        };
        chat.history_loaded(events, more);
        sync_outbox(&chat.items, session, &mut self.outbox);
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

    /// A transcript event: your verdict coming back drains its entry. `true` when it changed
    /// the chat.
    pub fn apply(&mut self, message: &ServerMessage) -> bool {
        let Some(chat) = self.chat.as_mut() else {
            return false;
        };
        if !chat.apply(message) {
            return false;
        }
        sync_outbox(&chat.items, &chat.session, &mut self.outbox);
        true
    }

    /// Pass clicked on the card: the message to send, or `None` (no review open, or its verdict
    /// is on its way already: a second click).
    pub fn pass(&mut self) -> Option<Outgoing> {
        let chat = self.chat.as_ref()?;
        let verdict = on_pass(&chat.items, &chat.session, &mut self.outbox)?;
        Some(Outgoing {
            session: chat.session.clone(),
            text: verdict.text,
            review: Some(verdict.review),
        })
    }

    /// A message from the composer: always sent. A verdict on the open review goes into the
    /// outbox too, unless one is on its way.
    pub fn send(&mut self, text: String) -> Option<Outgoing> {
        let chat = self.chat.as_ref()?;
        let review = on_send(&chat.items, &chat.session, &text, &mut self.outbox);
        Some(Outgoing {
            session: chat.session.clone(),
            text,
            review,
        })
    }

    /// `sent` didn't go: its verdict leaves the outbox of the session it was sent in.
    pub fn send_failed(&mut self, sent: &Outgoing) -> bool {
        sent.review
            .as_deref()
            .is_some_and(|review| self.outbox.failed(&sent.session, review))
    }

    /// The connection is back: what was sent before may have been lost with the server's inbox.
    pub fn reconnected(&mut self) {
        self.outbox.reconnected();
    }

    /// The verdict card while a review is open.
    pub fn verdict_card(&self) -> Option<VerdictCard> {
        let chat = self.chat.as_ref()?;
        verdict_card(&chat.items, &chat.session, &self.outbox)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use composer_protocol::{ChatMessage, Role};

    const REVIEW: &str = "url: site://coding/works/0014/review.md\n\n---\ntype: tui\n\
                          control: hunk\n---\n# Review 0014";

    /// The agent opening the Hunk review (the `open` call `call_id`), as the server sends it.
    fn review_opened(session: &str, seq: u64, call_id: &str) -> [ServerMessage; 2] {
        [
            ServerMessage::ToolCallStarted {
                session: session.into(),
                seq,
                call_id: call_id.into(),
                name: "open".into(),
                args: serde_json::json!({ "url": "site://coding/works/0014/review.md" }),
                parent: None,
            },
            ServerMessage::ToolResult {
                session: session.into(),
                seq: seq + 1,
                call_id: call_id.into(),
                result: REVIEW.into(),
                is_error: false,
            },
        ]
    }

    /// Show `session` (again), its history the review `c1` opened.
    fn show_review(model: &mut ChatModel, session: &str) {
        model.chat = None;
        model.open(session);
        model.history_loaded(session, &review_opened(session, 1, "c1"), false);
        assert!(model.verdict_card().is_some(), "the review is open");
    }

    fn reviewing(session: &str) -> ChatModel {
        let mut model = ChatModel::default();
        show_review(&mut model, session);
        model
    }

    /// The server's echo of a message you sent.
    fn echo(session: &str, seq: u64, text: &str) -> ServerMessage {
        ServerMessage::MessageDone {
            session: session.into(),
            seq,
            message: ChatMessage {
                role: Role::User,
                content: text.into(),
                thinking: None,
                tool_calls: vec![],
                pages: vec![],
                images: vec![],
                attachments: vec![],
                source: None,
            },
        }
    }

    fn sent(model: &ChatModel) -> Option<String> {
        model.verdict_card().and_then(|c| c.sent)
    }

    fn pass_of(session: &str, review: &str) -> Outgoing {
        Outgoing {
            session: session.into(),
            text: "pass".into(),
            review: Some(review.into()),
        }
    }

    #[test]
    fn two_clicks_one_send() {
        let mut model = reviewing("s");
        let sends: Vec<_> = (0..2).map(|_| model.pass()).collect();
        assert_eq!(
            sends,
            [Some(pass_of("s", "c1")), None],
            "two clicks, one send"
        );
        assert_eq!(sent(&model).as_deref(), Some("pass"), "shown at once");
    }

    #[test]
    fn an_applied_echo_empties_the_entry() {
        let mut model = reviewing("s");
        model.pass();
        // The running turn goes on: the entry stays.
        assert!(model.set_state(TurnState::Running));
        assert!(model.apply(&echo("s", 5, "looks odd")));
        assert_eq!(sent(&model).as_deref(), Some("pass"));
        assert!(model.apply(&echo("s", 6, "pass")));
        assert_eq!(model.verdict_card(), None, "the review is answered");
        // The entry went with the echo: failing that send finds nothing to take out.
        assert!(!model.send_failed(&pass_of("s", "c1")), "already drained");
    }

    #[test]
    fn an_idle_after_a_reconnect_drops_it() {
        let mut model = reviewing("s");
        model.pass();
        model.set_state(TurnState::Running);
        model.reconnected();
        assert_eq!(sent(&model).as_deref(), Some("pass"), "shown until we know");
        assert_eq!(model.pass(), None);
        assert!(model.set_state(TurnState::Idle));
        assert_eq!(sent(&model), None, "the server lost it");
        assert!(model.pass().is_some(), "Pass works again");
        // Sent after the reconnect: an idle leaves it.
        assert!(!model.set_state(TurnState::Idle));
        assert_eq!(sent(&model).as_deref(), Some("pass"));
    }

    #[test]
    fn a_failed_send_returns_to_the_session_it_went_to() {
        let mut model = reviewing("a");
        let pass = model.pass().expect("sent");
        // You moved on to another session, with its own review and verdict, before the
        // failure came back.
        show_review(&mut model, "b");
        let b = model.pass().expect("sent in b");
        assert!(model.send_failed(&pass), "a's entry goes");
        assert_eq!(sent(&model).as_deref(), Some("pass"), "b's stays");
        show_review(&mut model, "a");
        assert_eq!(sent(&model), None);
        assert_eq!(model.pass(), Some(pass), "Pass works again in a");
        assert!(model.send_failed(&b));
    }

    #[test]
    fn a_typed_verdict_takes_the_entry_and_a_second_is_sent_without_one() {
        let mut model = reviewing("s");
        let note = model.send("looks odd".into()).expect("sent");
        assert_eq!(note.review, None);
        let fail = model.send("fail: rename".into()).expect("sent");
        assert_eq!(fail.review.as_deref(), Some("c1"));
        let again = model
            .send("fail: and the docs".into())
            .expect("sent all the same");
        assert_eq!(again.review, None, "no second entry");
        assert_eq!(model.pass(), None);
        assert_eq!(sent(&model).as_deref(), Some("fail: rename"));
        assert!(!model.send_failed(&again), "its failure leaves the first");
        assert_eq!(sent(&model).as_deref(), Some("fail: rename"));
    }
}
