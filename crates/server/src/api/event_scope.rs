use composer_protocol::{Seq, ServerMessage};

/// Who an event is for.
pub enum EventScope<'a> {
    /// Clients attached to the session; `seq` is `None` for unsequenced events (approvals).
    Session { id: &'a str, seq: Option<Seq> },
    /// Clients subscribed to a matching URL.
    Page(&'a str),
    /// Every client.
    Global,
    /// Clients that opened the project (by name), and its apps' sockets.
    Project(&'a str),
    /// The connection of a desktop window (ticket 0177).
    Window(&'a str),
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
        // Unsequenced: to the clients attached to the session.
        // A window's TUI: to the window.
        TerminalOutput {
            window: Some(window),
            ..
        }
        | TerminalExited {
            window: Some(window),
            ..
        }
        | WindowChanged { window, .. } => EventScope::Window(window),
        DeskChanged { session, .. }
        | SelectionChanged { session, .. }
        | AppEvent { session, .. }
        | TerminalOutput { session, .. }
        | TerminalExited { session, .. } => EventScope::Session {
            id: session,
            seq: None,
        },
        ProjectChanged { project, .. } | OnOpenRan { project, .. } => EventScope::Project(project),
        SessionUpdated { .. }
        | WorkerStatus { .. }
        | Slots { .. }
        | ConfigReloaded { .. }
        | ReminderDue { .. } => EventScope::Global,
        Ok { .. } | Error { .. } => EventScope::Reply,
    }
}
