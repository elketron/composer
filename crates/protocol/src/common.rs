use serde::{Deserialize, Serialize};

/// Session ids are short hex strings, e.g. `a3f1`.
pub type SessionId = String;
/// A `site://` URL in string form.
pub type Url = String;
/// Position of an event in a session's transcript; strictly increasing.
pub type Seq = u64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SessionInfo {
    pub id: SessionId,
    /// Agent definition name (`site://agents/<agent>`).
    pub agent: String,
    /// Active project directory, if any.
    pub project: Option<String>,
    /// Set for worker sessions.
    pub ticket: Option<String>,
    /// The session that spawned this one (orchestrator of a worker).
    pub parent: Option<SessionId>,
    pub state: TurnState,
    /// Last seq written to the transcript.
    pub last_seq: Seq,
    /// Prompt tokens of the last model call: the context in use.
    #[serde(default)]
    pub context_tokens: u32,
    /// Start of the last message, for session lists.
    pub preview: Option<String>,
    /// RFC 3339.
    pub updated: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    Idle,
    /// Waiting for a model slot.
    Waiting,
    Running,
    /// Blocked on a recipe approval.
    AwaitingApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// Where a user-side message came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageSource {
    Tui,
    /// A micro app; `site` is the app's site.
    App {
        site: String,
    },
    /// A worker finished; sent to its orchestrator.
    Worker {
        session: SessionId,
    },
    Server,
    /// A reminder page came due (`action: agent`).
    Reminder {
        url: Url,
    },
}

/// A complete chat message, as stored in the transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// Reasoning tokens (assistant only). Not sent back to the model in later turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<Url>,
    /// Image URLs (site files), sent to the model as images.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<MessageSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approve,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PageChangeKind {
    Created,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ChangedBy {
    Agent,
    App,
    /// Changed on disk by something else (nvim, git, ...).
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkerState {
    Starting,
    Running,
    Done,
    Failed,
}
