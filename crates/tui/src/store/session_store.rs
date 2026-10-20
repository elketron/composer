use std::collections::HashMap;

use composer_protocol::{
    MessageSource, Role, Seq, ServerMessage, SessionInfo, TurnState, WorkerState,
};

use crate::store::types::{ChatItem, Streaming};

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
    /// Workers' last `worker.status`, by session id (seen since the TUI started).
    pub workers: HashMap<String, WorkerState>,
}

impl SessionStore {
    pub fn current_info(&self) -> Option<&SessionInfo> {
        let current = self.current.as_deref()?;
        self.sessions.iter().find(|s| s.id == current)
    }

    pub fn is_busy(&self) -> bool {
        self.current_info()
            .is_some_and(|s| s.state != TurnState::Idle)
    }

    /// The current session's orchestration: its orchestrator first, then the workers it
    /// spawned. Empty for a session without workers or orchestrator.
    pub fn orchestration(&self) -> Vec<&SessionInfo> {
        let Some(current) = self.current_info() else {
            return vec![];
        };
        let root = current.parent.as_deref().unwrap_or(&current.id);
        let workers = self
            .sessions
            .iter()
            .filter(|s| s.parent.as_deref() == Some(root));
        let root = self.sessions.iter().find(|s| s.id == root);
        let members: Vec<&SessionInfo> = root.into_iter().chain(workers).collect();
        if members.len() > 1 { members } else { vec![] }
    }

    /// What a session is doing, for lists: `running`, `waiting`, `approval`, `idle`, or a
    /// worker's `starting` / `done` / `failed` / `paused`.
    pub fn status(&self, session: &SessionInfo) -> &'static str {
        // Events seen since starting, else what the server remembers (`SessionInfo.worker`).
        let worker = self.workers.get(&session.id).or(session.worker.as_ref());
        match (session.state, worker) {
            // A finished worker is finished, whatever its last turn state said.
            (_, Some(WorkerState::Done)) => "done",
            (_, Some(WorkerState::Failed)) => "failed",
            (_, Some(WorkerState::Paused)) => "paused",
            (TurnState::Running, _) => "running",
            (TurnState::Waiting, _) => "waiting",
            (TurnState::AwaitingApproval, _) => "approval",
            (TurnState::Idle, Some(WorkerState::Starting)) => "starting",
            (TurnState::Idle, _) => "idle",
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
                        Some(MessageSource::App { site }) => Some(format!("{site} app")),
                        Some(MessageSource::Worker { session }) => {
                            Some(format!("worker #{session}"))
                        }
                        Some(MessageSource::Reminder { .. }) => Some("reminder".into()),
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
                ..
            } => self.items.push(ChatItem::Tool {
                call_id: call_id.clone(),
                name: name.clone(),
                summary: summarize_args(args),
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

/// The interesting argument of a tool call, on one line.
fn summarize_args(args: &serde_json::Value) -> String {
    let get = |key: &str| args.get(key).and_then(|v| v.as_str());
    if let (Some(site), Some(recipe)) = (get("site"), get("recipe")) {
        let extra: Vec<&str> = args["args"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a.as_str())
            .collect();
        return format!("{site} › {recipe} {}", extra.join(" "))
            .trim_end()
            .to_string();
    }
    for key in ["url", "query", "path", "ticket", "status", "site"] {
        if let Some(value) = get(key) {
            return value.to_string();
        }
    }
    match args {
        serde_json::Value::Object(map) if map.is_empty() => String::new(),
        other => other.to_string(),
    }
}
