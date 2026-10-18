use serde::{Deserialize, Serialize};

use crate::common::{ApprovalDecision, Seq, SessionId, Url, WorkerState};

/// A command from a client. The server answers with `ok` or `error` carrying the same `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ClientRequest {
    pub id: u64,
    #[serde(flatten)]
    pub command: ClientCommand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
}
