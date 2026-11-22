use composer_protocol::{MessageSource, Role, Seq, ServerMessage};

use crate::chat_item::{ChatItem, Streaming};
use crate::tool_text::{tool_detail, tool_summary};

/// One session's conversation, reduced from its transcript events (no UI): the items, the
/// message streaming in, and how far it has been read. A client opens a chat with the
/// session's last messages (`session.history`, [`Chat::history_loaded`]), then attaches since
/// the last of them and applies what follows ([`Chat::apply`]); older messages are put in front
/// as they are fetched ([`Chat::older_loaded`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chat {
    pub session: String,
    pub items: Vec<ChatItem>,
    pub streaming: Option<Streaming>,
    /// The last event applied: later ones only (a replay after reconnecting repeats some).
    pub last_seq: Seq,
    /// The oldest event shown; `more`: older ones remain.
    pub first_seq: Seq,
    pub more: bool,
    /// The last messages arrived (before that, the chat is still opening).
    pub loaded: bool,
    /// Turns finished since opening.
    pub turns_done: u64,
}

impl Chat {
    pub fn new(session: &str) -> Self {
        Self {
            session: session.to_string(),
            ..Self::default()
        }
    }

    /// The session's last messages: applied as if replayed.
    pub fn history_loaded(&mut self, events: &[ServerMessage], more: bool) {
        for event in events {
            self.apply(event);
        }
        self.first_seq = events.iter().find_map(seq_of).unwrap_or(self.first_seq);
        self.more = more;
        self.loaded = true;
    }

    /// Messages before the oldest shown, put in front.
    pub fn older_loaded(&mut self, events: &[ServerMessage], more: bool) {
        let Some(first) = events.iter().find_map(seq_of) else {
            self.more = false;
            return;
        };
        let mut older = Chat::new(&self.session);
        for event in events {
            older.apply(event);
        }
        older.items.append(&mut self.items);
        self.items = older.items;
        self.first_seq = first;
        self.more = more;
    }

    /// Applies a transcript event of this session; false for another session's, an older one,
    /// or something that isn't a transcript event.
    pub fn apply(&mut self, message: &ServerMessage) -> bool {
        use ServerMessage::*;
        let Some((session, seq)) = session_seq(message) else {
            return false;
        };
        if session != self.session || seq <= self.last_seq {
            return false;
        }
        self.last_seq = seq;
        match message {
            ThinkingDelta { text, .. } => self
                .streaming
                .get_or_insert_default()
                .thinking
                .push_str(text),
            MessageDelta { text, .. } => self
                .streaming
                .get_or_insert_default()
                .content
                .push_str(text),
            MessageDone { message, .. } => match message.role {
                Role::User => self.items.push(ChatItem::User {
                    text: message.content.clone(),
                    source: source_label(message.source.as_ref()),
                    files: message.attachments.iter().map(|a| a.name.clone()).collect(),
                }),
                Role::Assistant => {
                    self.streaming = None;
                    if let Some(thinking) =
                        message.thinking.as_ref().filter(|t| !t.trim().is_empty())
                    {
                        self.items.push(ChatItem::Thinking {
                            text: thinking.clone(),
                        });
                    }
                    if !message.content.trim().is_empty() {
                        self.items.push(ChatItem::Text {
                            text: message.content.clone(),
                        });
                    }
                }
                Role::System if message.source == Some(MessageSource::System1) => {
                    self.items.push(ChatItem::System1 {
                        text: message.content.clone(),
                    })
                }
                Role::System | Role::Tool => self.items.push(ChatItem::System {
                    text: message.content.clone(),
                }),
            },
            ToolCallStarted {
                call_id,
                name,
                args,
                parent,
                ..
            } => self.items.push(ChatItem::Tool {
                call_id: call_id.clone(),
                name: name.clone(),
                summary: tool_summary(name, args),
                detail: tool_detail(name, args),
                nested: parent.is_some(),
                result: None,
                is_error: false,
            }),
            ToolResult {
                call_id,
                result,
                is_error,
                ..
            } => {
                let tool = self
                    .items
                    .iter_mut()
                    .rev()
                    .find(|i| matches!(i, ChatItem::Tool { call_id: c, .. } if c == call_id));
                if let Some(ChatItem::Tool {
                    result: r,
                    is_error: e,
                    ..
                }) = tool
                {
                    *r = Some(result.clone());
                    *e = *is_error;
                }
            }
            SessionCompacted {
                summary, messages, ..
            } => self.items.push(ChatItem::Compacted {
                summary: summary.clone(),
                messages: *messages,
            }),
            TurnDone { cancelled, .. } => {
                self.turns_done += 1;
                self.streaming = None;
                if *cancelled {
                    self.items.push(ChatItem::System {
                        text: "cancelled".into(),
                    });
                }
            }
            _ => {}
        }
        true
    }
}

/// Who sent a user message, when not you.
fn source_label(source: Option<&MessageSource>) -> Option<String> {
    match source? {
        MessageSource::Watch { .. } => Some("watch".into()),
        MessageSource::Process { .. } => Some("process".into()),
        MessageSource::Mcp { server } => Some(server.clone()),
        _ => None,
    }
}

fn session_seq(message: &ServerMessage) -> Option<(&str, Seq)> {
    use ServerMessage::*;
    match message {
        TurnStarted { session, seq }
        | TurnQueued { session, seq, .. }
        | TurnDone { session, seq, .. }
        | ThinkingDelta { session, seq, .. }
        | MessageDelta { session, seq, .. }
        | MessageDone { session, seq, .. }
        | ToolCallStarted { session, seq, .. }
        | ToolResult { session, seq, .. }
        | AgentNavigated { session, seq, .. }
        | SessionCompacted { session, seq, .. } => Some((session, *seq)),
        _ => None,
    }
}

fn seq_of(message: &ServerMessage) -> Option<Seq> {
    session_seq(message).map(|(_, seq)| seq)
}

#[cfg(test)]
mod tests {
    use composer_protocol::{ChatMessage, Role};

    use super::*;

    fn done(seq: Seq, role: Role, content: &str) -> ServerMessage {
        ServerMessage::MessageDone {
            session: "a".into(),
            seq,
            message: ChatMessage {
                role,
                content: content.into(),
                thinking: None,
                tool_calls: vec![],
                pages: vec![],
                images: vec![],
                attachments: vec![],
                source: None,
            },
        }
    }

    fn delta(seq: Seq, text: &str) -> ServerMessage {
        ServerMessage::MessageDelta {
            session: "a".into(),
            seq,
            text: text.into(),
        }
    }

    #[test]
    fn multiline_thinking_stays_intact_when_the_answer_starts_and_settles() {
        let mut chat = Chat::new("a");
        let mut thinking = String::new();
        for (i, text) in [
            "Plan the scene.",
            "\n\n",
            "Check the objects.",
            "\nThen render.",
        ]
        .iter()
        .enumerate()
        {
            thinking.push_str(text);
            chat.apply(&ServerMessage::ThinkingDelta {
                session: "a".into(),
                seq: i as Seq + 1,
                text: (*text).into(),
            });
            assert_eq!(chat.streaming.as_ref().unwrap().thinking, thinking);
        }
        // The streamed row's key uses the index the finished thinking will occupy.
        let thinking_index = chat.items.len();
        chat.apply(&delta(5, "Here is the scene."));
        assert_eq!(chat.streaming.as_ref().unwrap().thinking, thinking);
        assert_eq!(chat.items.len(), thinking_index);
        let mut finished = done(6, Role::Assistant, "Here is the scene.");
        if let ServerMessage::MessageDone { message, .. } = &mut finished {
            message.thinking = Some(thinking.clone());
        }
        chat.apply(&finished);
        assert!(chat.streaming.is_none());
        assert_eq!(
            chat.items[thinking_index],
            ChatItem::Thinking { text: thinking }
        );
        assert_eq!(
            chat.items[thinking_index + 1],
            ChatItem::Text {
                text: "Here is the scene.".into()
            }
        );
    }

    #[test]
    fn a_turn_streams_then_settles() {
        let mut chat = Chat::new("a");
        chat.history_loaded(&[done(1, Role::User, "hi")], false);
        assert!(chat.loaded && chat.first_seq == 1);
        chat.apply(&ServerMessage::TurnStarted {
            session: "a".into(),
            seq: 2,
        });
        chat.apply(&delta(3, "Hel"));
        chat.apply(&delta(4, "lo"));
        assert_eq!(chat.streaming.as_ref().unwrap().content, "Hello");
        chat.apply(&ServerMessage::ToolCallStarted {
            session: "a".into(),
            seq: 5,
            call_id: "c1".into(),
            name: "Bash".into(),
            args: serde_json::json!({"command": "ls -la\nmore"}),
            parent: None,
        });
        chat.apply(&ServerMessage::ToolResult {
            session: "a".into(),
            seq: 6,
            call_id: "c1".into(),
            result: "x".into(),
            is_error: true,
        });
        chat.apply(&done(7, Role::Assistant, "Hello"));
        chat.apply(&ServerMessage::TurnDone {
            session: "a".into(),
            seq: 8,
            cancelled: false,
        });
        assert!(chat.streaming.is_none());
        assert_eq!(chat.turns_done, 1);
        assert_eq!(
            chat.items[1],
            ChatItem::Tool {
                call_id: "c1".into(),
                name: "Bash".into(),
                summary: "ls -la".into(),
                detail: None,
                nested: false,
                result: Some("x".into()),
                is_error: true,
            }
        );
        assert_eq!(
            chat.items[2],
            ChatItem::Text {
                text: "Hello".into()
            }
        );
    }

    #[test]
    fn a_system1_decision_is_its_own_item() {
        let mut chat = Chat::new("a");
        let mut line = done(1, Role::System, "system-1 rejected `rm -rf /` — no");
        if let ServerMessage::MessageDone { message, .. } = &mut line {
            message.source = Some(MessageSource::System1);
        }
        chat.apply(&line);
        chat.apply(&done(2, Role::System, "Turn stopped: x"));
        assert_eq!(
            chat.items,
            vec![
                ChatItem::System1 {
                    text: "system-1 rejected `rm -rf /` — no".into()
                },
                ChatItem::System {
                    text: "Turn stopped: x".into()
                },
            ]
        );
    }

    #[test]
    fn replays_and_other_sessions_are_ignored() {
        let mut chat = Chat::new("a");
        assert!(chat.apply(&done(1, Role::User, "one")));
        assert!(!chat.apply(&done(1, Role::User, "one")), "a replay");
        let other = ServerMessage::MessageDone {
            session: "b".into(),
            seq: 9,
            message: match done(9, Role::User, "x") {
                ServerMessage::MessageDone { message, .. } => message,
                _ => unreachable!(),
            },
        };
        assert!(!chat.apply(&other));
        assert_eq!(chat.items.len(), 1);
    }

    #[test]
    fn older_messages_go_in_front() {
        let mut chat = Chat::new("a");
        chat.history_loaded(&[done(5, Role::User, "new")], true);
        chat.older_loaded(&[done(2, Role::User, "old")], false);
        assert_eq!(
            chat.items.first(),
            Some(&ChatItem::User {
                text: "old".into(),
                source: None,
                files: vec![]
            })
        );
        assert_eq!((chat.first_seq, chat.more, chat.last_seq), (2, false, 5));
    }
}
