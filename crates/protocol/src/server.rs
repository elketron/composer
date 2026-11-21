use serde::{Deserialize, Serialize};

use crate::common::{
    ApprovalKind, ChangedBy, ChatMessage, MessageSource, ModelInfo, PageChangeKind, Seq, SessionId,
    SessionInfo, ToolInfo, Url,
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
        /// The call this one runs inside of: a sub-agent's (Claude Code's `Agent` tool).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
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

    /// Something waits for the user's yes: a recipe's first run (`site`, `recipe`, `args`,
    /// `hash`), a ticket merge an agent asked for, or a tool an external harness wants. `body`
    /// is what to look at (the recipe, the changes, the tool's input).
    #[serde(rename = "approval.request")]
    ApprovalRequest {
        request: String,
        session: SessionId,
        #[serde(default)]
        kind: ApprovalKind,
        /// One line saying what is asked (empty for recipes: clients show site and recipe).
        #[serde(default)]
        title: String,
        site: String,
        recipe: String,
        args: Vec<String>,
        body: String,
        hash: String,
        /// For a tool: what `always` would allow from then on in this project (`Bash(npm:*)`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule: Option<String>,
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

    /// A terminal's output (base64 bytes), to the session's clients.
    #[serde(rename = "terminal.output")]
    TerminalOutput {
        session: SessionId,
        terminal: String,
        data: String,
    },
    /// A terminal ended (its shell exited, it was closed, or it was idle too long).
    #[serde(rename = "terminal.exited")]
    TerminalExited {
        session: SessionId,
        terminal: String,
    },
    /// A session's desk changed (`by` the user or the agent): to the clients attached to it.
    #[serde(rename = "desk.changed")]
    DeskChanged {
        session: SessionId,
        desk: crate::Desk,
        by: crate::DeskBy,
        /// The tab the agent opened or pointed at: a client following the agent shows it
        /// (`desk.focus`), one not following badges it (it stays `unseen`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opened: Option<String>,
    },

    /// `config.toml` changed and the server read it again. `error`: it doesn't parse, and the
    /// server keeps running with the config it had.
    #[serde(rename = "config.reloaded")]
    ConfigReloaded {
        restart_needed: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Model slot usage, shared by all sessions.
    #[serde(rename = "slots")]
    Slots { used: u32, total: u32, waiting: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandResult {
    Done,
    /// Done, with what the user should know (`site.set`).
    Note {
        text: String,
    },
    Session {
        session: SessionInfo,
    },
    Sessions {
        sessions: Vec<SessionInfo>,
    },
    Tools {
        tools: Vec<ToolInfo>,
    },
    ToolOutput {
        output: String,
        is_error: bool,
        /// Images in the result (an MCP screenshot), as data URLs.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<String>,
    },
    /// `model.list`: what the provider lists, and the model sessions use unless set.
    Models {
        default_model: String,
        models: Vec<ModelInfo>,
    },
    /// `session.history`'s events, oldest first; `more`: older ones remain.
    History {
        events: Vec<ServerMessage>,
        more: bool,
    },
    McpServers {
        servers: Vec<crate::http::McpServerInfo>,
    },
    /// `terminal.open` / `terminal.read`: the terminal and its scrollback (base64).
    Terminal {
        terminal: String,
        scrollback: String,
    },
    /// `desk.*`: the session's desk now.
    Desk {
        desk: crate::Desk,
    },
    ConfigReloaded {
        /// False: the file says what the server already runs with.
        changed: bool,
        /// Changed settings that only take effect after a restart (`provider`, `agent.slots`, …).
        restart_needed: Vec<String>,
    },
}
