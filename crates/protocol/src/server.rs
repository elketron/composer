use serde::{Deserialize, Serialize};

use crate::common::{
    ChangedBy, ChatMessage, MessageSource, PageChangeKind, Seq, SessionId, SessionInfo, Url,
    WorkerState,
};

/// Everything the server sends over `/ws`.
///
/// Session events carry `session` and `seq` and are stored in the session's
/// transcript, so attaching is replaying them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type")]
pub enum ServerMessage {
    // ---- command replies ----
    #[serde(rename = "ok")]
    Ok { id: u64, result: CommandResult },

    #[serde(rename = "error")]
    Error { id: Option<u64>, message: String },

    // ---- session events ----
    #[serde(rename = "turn.started")]
    TurnStarted { session: SessionId, seq: Seq },

    /// A message arrived while a turn was running; it starts the next turn.
    #[serde(rename = "turn.queued")]
    TurnQueued {
        session: SessionId,
        seq: Seq,
        source: MessageSource,
    },

    #[serde(rename = "turn.done")]
    TurnDone {
        session: SessionId,
        seq: Seq,
        cancelled: bool,
    },

    #[serde(rename = "thinking.delta")]
    ThinkingDelta {
        session: SessionId,
        seq: Seq,
        text: String,
    },

    #[serde(rename = "message.delta")]
    MessageDelta {
        session: SessionId,
        seq: Seq,
        text: String,
    },

    /// A complete message (user or assistant). Deltas before it are superseded.
    #[serde(rename = "message.done")]
    MessageDone {
        session: SessionId,
        seq: Seq,
        message: ChatMessage,
    },

    #[serde(rename = "tool.call")]
    ToolCallStarted {
        session: SessionId,
        seq: Seq,
        call_id: String,
        name: String,
        args: serde_json::Value,
    },

    #[serde(rename = "tool.result")]
    ToolResult {
        session: SessionId,
        seq: Seq,
        call_id: String,
        result: String,
        is_error: bool,
    },

    /// Older messages were replaced by `summary` (after a memory pass).
    #[serde(rename = "session.compacted")]
    SessionCompacted {
        session: SessionId,
        seq: Seq,
        summary: String,
        /// How many history messages the summary replaced.
        messages: u32,
    },

    /// The agent opened a page; the TUI's viewer follows it.
    #[serde(rename = "agent.navigated")]
    AgentNavigated {
        session: SessionId,
        seq: Seq,
        url: Url,
    },

    #[serde(rename = "approval.request")]
    ApprovalRequest {
        request: String,
        session: SessionId,
        site: String,
        recipe: String,
        args: Vec<String>,
        body: String,
        hash: String,
    },

    /// Someone answered an approval request; clients close their prompt.
    #[serde(rename = "approval.resolved")]
    ApprovalResolved {
        request: String,
        session: SessionId,
        approved: bool,
    },

    #[serde(rename = "session.updated")]
    SessionUpdated { session: SessionInfo },

    // ---- global events ----
    #[serde(rename = "page.changed")]
    PageChanged {
        url: Url,
        kind: PageChangeKind,
        by: ChangedBy,
    },

    #[serde(rename = "worker.status")]
    WorkerStatus {
        worker: SessionId,
        ticket: String,
        parent: SessionId,
        state: WorkerState,
    },

    /// Model slot usage, shared by all sessions and workers.
    #[serde(rename = "slots")]
    Slots { used: u32, total: u32, waiting: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandResult {
    Done,
    Session { session: SessionInfo },
    Sessions { sessions: Vec<SessionInfo> },
    SlotGranted { grant: u64 },
    Approval { approved: bool },
}
