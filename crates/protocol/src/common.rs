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
    /// What its runs are: a single shot, a workflow run or an agentic session.
    #[serde(default)]
    pub kind: RunKind,
    /// The site the session is in (`set_site`), if any: its page is the agent's instructions,
    /// and its `folder:` is where terminals start and what the changes view diffs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    /// The workflow folder inside its site whose instructions (`CONTEXT.md`, else `index.md`)
    /// its runs follow, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// The session that spawned this one (`spawn_run`), if any.
    pub parent: Option<SessionId>,
    /// The outside agent (an `[acp.<name>]` of config.toml) that runs its turns; `None`:
    /// composer's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    pub state: TurnState,
    /// Last seq written to the transcript.
    pub last_seq: Seq,
    /// Prompt tokens of the last model call: the context in use.
    #[serde(default)]
    pub context_tokens: u32,
    /// The model its turns use (`model.set`); None: the server's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Start of the last message, for session lists.
    pub preview: Option<String>,
    /// RFC 3339.
    pub updated: String,
}

/// What a run is, and so which tools it has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    /// One model call without tools: the instructions and the input in, the answer out.
    SingleShot,
    /// A loop with read-only tools that ends when the model calls `finish(value)`; the value is
    /// its result.
    Workflow,
    /// A conversation with every tool.
    #[default]
    Agentic,
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
    Server,
    /// Files the session watches (the `watch` tool) changed; `pattern` is what it watched.
    Watch {
        pattern: String,
    },
    /// A background process the agent started (`process_start`) exited.
    Process {
        terminal: String,
        command: String,
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
    /// Files uploaded to the session (`POST /api/sessions/{id}/attachments`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
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

/// A file uploaded to a session, as messages refer to it. The server checks `file` against the
/// session's attachments and fills in the rest itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Attachment {
    /// The stored file in the session's attachments folder: `<hash>-<name>`.
    pub file: String,
    /// The name it was uploaded with (sanitized).
    pub name: String,
    pub mime: String,
    pub size: u64,
}

/// A model the provider lists (`GET /models`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ModelInfo {
    pub id: String,
    /// Its display name, when the provider gives one (OpenRouter does).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Context window in tokens, when listed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<u32>,
}

/// A tool an agent may call: what the model (or an external harness) sees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    /// JSON schema of the arguments object.
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approve,
    Deny,
    /// Approve, and don't ask again for this rule in this folder (tool requests only).
    Always,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    #[default]
    Recipe,
    Tool,
    /// A call to an MCP server's tool (`rule`: the tool, for `always`; it holds everywhere).
    Mcp,
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
    /// Changed on disk by something else (nvim, git, ...).
    External,
}
