use composer_protocol::{MessageSource, Role, Seq, ServerMessage, SessionInfo, TurnState};

use crate::store::types::{ChatItem, Streaming};
use crate::tool_text::{tool_detail, tool_summary};

/// Sessions and the chat of the current one.
#[derive(Debug, Default)]
pub struct SessionStore {
    pub sessions: Vec<SessionInfo>,
    pub current: Option<String>,
    pub items: Vec<ChatItem>,
    pub streaming: Option<Streaming>,
    /// Sessions visited before the current one (`ctrl-o` goes back).
    pub jumplist: Vec<String>,
    pub last_seq: Seq,
    /// Turns finished in the current session since attaching (scripts wait on this).
    pub turns_done: u64,
    /// The site the session picker is narrowed to (`:scope`).
    pub scope: Option<String>,
    /// The current session's last messages arrived (it opens on its end), and it is attached.
    pub loaded: bool,
    /// The oldest event shown; `more`: older ones remain, fetched as you scroll up to it.
    pub first_seq: Seq,
    pub more: bool,
    pub loading_older: bool,
}

impl SessionStore {
    /// Whether a session is in the scope (always, without one): it is in that site.
    pub fn in_scope(&self, session: &SessionInfo) -> bool {
        self.scope
            .as_ref()
            .is_none_or(|scope| session.site.as_ref() == Some(scope))
    }

    pub fn current_info(&self) -> Option<&SessionInfo> {
        let current = self.current.as_deref()?;
        self.sessions.iter().find(|s| s.id == current)
    }

    pub fn is_busy(&self) -> bool {
        self.current_info()
            .is_some_and(|s| s.state != TurnState::Idle)
    }

    /// What a session is doing, for lists: `running`, `waiting`, `approval` or `idle`.
    pub fn status(&self, session: &SessionInfo) -> &'static str {
        match session.state {
            TurnState::Running => "running",
            TurnState::Waiting => "waiting",
            TurnState::AwaitingApproval => "approval",
            TurnState::Idle => "idle",
        }
    }

    /// Switches to `id`; the chat refills from the attach replay.
    pub fn begin_attach(&mut self, id: &str, remember: bool) {
        if self.current.as_deref() == Some(id) {
            return;
        }
        if remember && let Some(previous) = self.current.take() {
            self.jumplist.push(previous);
        }
        self.current = Some(id.to_string());
        self.items.clear();
        self.streaming = None;
        self.last_seq = 0;
        self.loaded = false;
        self.first_seq = 0;
        self.more = false;
        self.loading_older = false;
    }

    /// The session's last messages (`session.history`): shown as if replayed; then it attaches
    /// since the last of them.
    pub fn history_loaded(&mut self, events: &[ServerMessage], more: bool) {
        for event in events {
            self.apply(event);
        }
        self.first_seq = events.iter().find_map(seq_of).unwrap_or(0);
        self.more = more;
        self.loaded = true;
    }

    /// Messages before the oldest shown, put in front.
    pub fn older_loaded(&mut self, events: &[ServerMessage], more: bool) {
        self.loading_older = false;
        let Some(first) = events.iter().find_map(seq_of) else {
            self.more = false;
            return;
        };
        let mut older = SessionStore {
            current: self.current.clone(),
            ..SessionStore::default()
        };
        for event in events {
            older.apply(event);
        }
        older.items.append(&mut self.items);
        self.items = older.items;
        self.first_seq = first;
        self.more = more;
    }

    pub fn upsert(&mut self, info: SessionInfo) {
        match self.sessions.iter_mut().find(|s| s.id == info.id) {
            Some(existing) => *existing = info,
            None => self.sessions.insert(0, info),
        }
    }

    /// Applies a session event for the current session. Returns false if it was for another one.
    pub fn apply(&mut self, message: &ServerMessage) -> bool {
        use ServerMessage::*;
        let (session, seq) = match message {
            TurnStarted { session, seq }
            | TurnQueued { session, seq, .. }
            | TurnDone { session, seq, .. }
            | ThinkingDelta { session, seq, .. }
            | MessageDelta { session, seq, .. }
            | MessageDone { session, seq, .. }
            | ToolCallStarted { session, seq, .. }
            | ToolResult { session, seq, .. }
            | AgentNavigated { session, seq, .. }
            | SessionCompacted { session, seq, .. } => (session, *seq),
            _ => return false,
        };
        if self.current.as_deref() != Some(session.as_str()) || seq <= self.last_seq {
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
                    source: match &message.source {
                        Some(MessageSource::Watch { .. }) => Some("watch".into()),
                        Some(MessageSource::Process { .. }) => Some("process".into()),
                        _ => None,
                    },
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
            TurnDone {
                cancelled: true, ..
            } => {
                self.turns_done += 1;
                self.streaming = None;
                self.items.push(ChatItem::System {
                    text: "cancelled".into(),
                });
            }
            TurnDone { .. } => {
                self.turns_done += 1;
                self.streaming = None;
            }
            _ => {}
        }
        true
    }
}
/// A transcript event's seq.
fn seq_of(message: &ServerMessage) -> Option<Seq> {
    use ServerMessage::*;
    match message {
        TurnStarted { seq, .. }
        | TurnQueued { seq, .. }
        | TurnDone { seq, .. }
        | ThinkingDelta { seq, .. }
        | MessageDelta { seq, .. }
        | MessageDone { seq, .. }
        | ToolCallStarted { seq, .. }
        | ToolResult { seq, .. }
        | AgentNavigated { seq, .. }
        | SessionCompacted { seq, .. } => Some(*seq),
        _ => None,
    }
}
