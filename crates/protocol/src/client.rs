use serde::{Deserialize, Serialize};

use crate::common::{ApprovalDecision, Attachment, Seq, SessionId, Url, WorkerState};

/// A command from a client. The server answers with `ok` or `error` carrying the same `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientRequest {
    pub id: u64,
    #[serde(flatten)]
    pub command: ClientCommand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type")]
pub enum ClientCommand {
    #[serde(rename = "session.create")]
    SessionCreate {
        agent: String,
        project: Option<String>,
    },

    #[serde(rename = "session.list")]
    SessionList,

    /// Replays events after `since` (all if `None`), then streams live events.
    #[serde(rename = "session.attach")]
    SessionAttach {
        session: SessionId,
        since: Option<Seq>,
    },

    #[serde(rename = "session.detach")]
    SessionDetach { session: SessionId },

    /// Queued if the agent is mid-turn.
    #[serde(rename = "message.send")]
    MessageSend {
        session: SessionId,
        text: String,
        #[serde(default)]
        pages: Vec<Url>,
        #[serde(default)]
        images: Vec<Url>,
        /// Uploaded first, then referred to here.
        #[serde(default)]
        attachments: Vec<Attachment>,
    },

    /// Compact the session now: save memory, then summarize older messages.
    #[serde(rename = "session.compact")]
    SessionCompact { session: SessionId },

    #[serde(rename = "turn.cancel")]
    TurnCancel { session: SessionId },

    #[serde(rename = "project.set")]
    ProjectSet {
        session: SessionId,
        path: Option<String>,
    },

    /// The external harnesses of a project (a folder, `~/x`, or a name in the projects dir).
    #[serde(rename = "harness.list")]
    HarnessList { project: String },

    /// Turns a harness (`claude-code`) or one of its models (`claude-code:opus`) on or off for a
    /// project. Off stops its running tickets there; on lets the ones it stopped go on.
    #[serde(rename = "harness.set")]
    HarnessSet {
        project: String,
        entry: String,
        on: bool,
    },

    /// What is going on in a project (a name in the projects directory, or a path); the reply is
    /// `project`.
    #[serde(rename = "project.overview")]
    ProjectOverview { project: String },

    /// A ticket's changes against the branch it would be merged into; the reply is `changes`.
    #[serde(rename = "ticket.changes")]
    TicketChanges { ticket: String },

    /// Merges a ticket's branch into the project's checked-out branch and removes its
    /// worktree; the reply is `merged`.
    #[serde(rename = "ticket.merge")]
    TicketMerge { ticket: String },

    /// Removes a ticket's worktree and branch without merging.
    #[serde(rename = "ticket.discard")]
    TicketDiscard { ticket: String },

    #[serde(rename = "approval.respond")]
    ApprovalRespond {
        request: String,
        decision: ApprovalDecision,
    },

    /// Page change notifications for URLs matching any glob (`site://design/**`).
    #[serde(rename = "subscribe")]
    Subscribe { urls: Vec<String> },

    // ---- from worker processes to their parent server ----
    /// This connection is the worker process running `session`.
    #[serde(rename = "worker.hello")]
    WorkerHello { session: SessionId },

    /// An event of the worker's session; the parent re-sequences and broadcasts it.
    #[serde(rename = "worker.event")]
    WorkerEvent {
        event: Box<crate::server::ServerMessage>,
    },

    /// Waits for a model slot; the reply is `slot_granted`. Held until released or disconnect.
    #[serde(rename = "slot.acquire")]
    SlotAcquire,

    #[serde(rename = "slot.release")]
    SlotRelease { grant: u64 },

    /// Asks the user (through the parent's clients) to approve a recipe version.
    #[serde(rename = "approval.ask")]
    ApprovalAsk {
        site: String,
        recipe: String,
        args: Vec<String>,
        body: String,
        hash: String,
    },

    /// The worker is done with its ticket.
    #[serde(rename = "worker.finish")]
    WorkerFinish { state: WorkerState, summary: String },

    // ---- from harness tool servers (`composer mcp`) ----
    /// The tools a running worker's agent may call; the reply is `tools`.
    #[serde(rename = "tool.list")]
    ToolList { session: SessionId },

    /// Runs one of them in the running worker's session; the reply is `tool_output`.
    #[serde(rename = "tool.call")]
    ToolCall {
        session: SessionId,
        name: String,
        #[serde(default)]
        args: serde_json::Value,
    },
}
