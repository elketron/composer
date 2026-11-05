use std::fs::File;

use composer_protocol::{Seq, ServerMessage, TurnState};

/// Mutable part of a transcript, behind its lock.
#[derive(Debug)]
pub struct TranscriptInner {
    pub next_seq: Seq,
    pub file: File,
    /// The persisted events, as parsed at open plus everything appended here: reads serve
    /// from this instead of re-parsing the file.
    pub events: Vec<ServerMessage>,
}

/// Approval requests open on a session.
#[derive(Debug)]
pub struct OpenApprovals {
    pub count: usize,
    /// The state to go back to when the last one closes.
    pub after: TurnState,
}

/// One model call's result.
#[derive(Debug, Default)]
pub struct StepOutput {
    pub content: String,
    pub thinking: String,
    pub tool_calls: Vec<crate::provider::ModelToolCall>,
    pub usage: Option<crate::provider::Usage>,
    /// The user cancelled while it streamed; the output is partial.
    pub cancelled: bool,
}
