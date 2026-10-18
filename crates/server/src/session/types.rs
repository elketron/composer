use std::fs::File;

use composer_protocol::Seq;

/// Mutable part of a transcript, behind its lock.
#[derive(Debug)]
pub struct TranscriptInner {
    pub next_seq: Seq,
    pub file: File,
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
