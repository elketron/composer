use composer_protocol::{Seq, ServerMessage};

/// Who an event is for.
pub enum EventScope<'a> {
    /// Clients attached to the session; `seq` is `None` for unsequenced events (approvals).
    Session { id: &'a str, seq: Option<Seq> },
    /// Clients subscribed to a matching URL.
    Page(&'a str),
    /// Every client.
    Global,
    /// A reply to one client's command; never broadcast.
    Reply,
}

pub fn scope(message: &ServerMessage) -> EventScope<'_> {
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
        | SessionCompacted { session, seq, .. } => EventScope::Session {
            id: session,
            seq: Some(*seq),
        },
        // Every client: a worker's recipe needs your answer wherever you are.
        ApprovalRequest { .. } | ApprovalResolved { .. } => EventScope::Global,
        PageChanged { url, .. } => EventScope::Page(url),
        SessionUpdated { .. } | WorkerStatus { .. } | Slots { .. } => EventScope::Global,
        Ok { .. } | Error { .. } => EventScope::Reply,
    }
}
