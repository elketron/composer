use serde::{Deserialize, Serialize};

use crate::common::{ApprovalDecision, Attachment, RunKind, Seq, SessionId, Url};

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
        /// Default: an agentic session.
        #[serde(default)]
        kind: RunKind,
        /// The model its runs use; default: the server's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
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

    /// The transcript's last `messages` messages before `before` (the end if `None`), with the
    /// events between them; the reply is `history`. A client opens a long chat on its end this
    /// way (then attaches `since` the last of them) and fetches older ones as you scroll up.
    #[serde(rename = "session.history")]
    SessionHistory {
        session: SessionId,
        before: Option<Seq>,
        messages: u32,
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

    /// Stops the session's running turn. `seen`: the last event of the session the client had
    /// when the user asked. A turn that started after it is one they hadn't seen (a second Stop
    /// landing on the message they had queued), and keeps running. Without it, whatever runs
    /// stops. The reply is `done` either way.
    #[serde(rename = "turn.cancel")]
    TurnCancel {
        session: SessionId,
        seen: Option<Seq>,
    },

    /// Enters the site `site` (its page and tools) for a session, or leaves the current one with
    /// `None`. A running turn goes on as it was; the change applies from the session's next turn.
    /// The session's new state arrives as `session.updated`; the reply is `note`: what happened.
    /// An error when the site does not exist.
    #[serde(rename = "site.set")]
    SiteSet {
        session: SessionId,
        site: Option<String>,
    },

    /// The models the provider lists; the reply is `models`.
    #[serde(rename = "model.list")]
    ModelList,

    /// The model a session's turns use; `None`: the server's again. The reply is `session`.
    #[serde(rename = "model.set")]
    ModelSet {
        session: SessionId,
        model: Option<String>,
    },

    /// Opens a terminal for a session (ticket 0176): a shell, in the
    /// session's project (or the sites); a tab on its desk. With the `id` of one that ended (idle),
    /// a fresh shell where it was. The reply is `terminal`.
    #[serde(rename = "terminal.open")]
    TerminalOpen {
        session: SessionId,
        /// (Not `id`: that is the request's.)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<String>,
        #[serde(default = "default_cols")]
        cols: u16,
        #[serde(default = "default_rows")]
        rows: u16,
    },

    /// Typed into a terminal (text, escape sequences included). The reply is `done`.
    #[serde(rename = "terminal.input")]
    TerminalInput { terminal: String, data: String },

    /// A terminal's new size. The reply is `done`.
    #[serde(rename = "terminal.resize")]
    TerminalResize {
        terminal: String,
        cols: u16,
        rows: u16,
    },

    /// A terminal's scrollback (a client that joins late); the reply is `terminal`.
    #[serde(rename = "terminal.read")]
    TerminalRead { terminal: String },

    /// Ends a terminal. The reply is `done`.
    #[serde(rename = "terminal.close")]
    TerminalClose { terminal: String },

    /// A session's desk; the reply is `desk`.
    #[serde(rename = "desk.get")]
    DeskGet { session: SessionId },

    /// Opens a tab on a session's desk (an equal tab already there is focused, not doubled);
    /// `focus`: make it the shown one (default). The reply is `desk`.
    #[serde(rename = "desk.open")]
    DeskOpen {
        session: SessionId,
        tab: crate::DeskTab,
        #[serde(default = "yes")]
        focus: bool,
    },

    /// Closes a tab (by id); the reply is `desk`.
    #[serde(rename = "desk.close")]
    DeskClose { session: SessionId, tab: String },

    /// Shows a tab (by id); the reply is `desk`.
    #[serde(rename = "desk.focus")]
    DeskFocus { session: SessionId, tab: String },

    /// Moves a tab (by id) to position `to`; the reply is `desk`.
    #[serde(rename = "desk.move")]
    DeskMove {
        session: SessionId,
        tab: String,
        to: usize,
    },

    /// The MCP servers in `config.toml`, connecting those that are on (so it is also a test of
    /// them); the reply is `mcp_servers`.
    #[serde(rename = "mcp.list")]
    McpList,

    /// Reads `config.toml` again now (the server also does when the file changes); the reply is
    /// `config_reloaded`. A file that doesn't parse is an error and the running config stays.
    #[serde(rename = "config.reload")]
    ConfigReload,

    #[serde(rename = "approval.respond")]
    ApprovalRespond {
        request: String,
        decision: ApprovalDecision,
    },

    /// Page change notifications for URLs matching any glob (`site://design/**`).
    #[serde(rename = "subscribe")]
    Subscribe { urls: Vec<String> },

    // ---- from the MCP bridge (`composer mcp`) ----
    /// The tools a session's agent may call; the reply is `tools`.
    #[serde(rename = "tool.list")]
    ToolList { session: SessionId },

    /// Runs one of them in the session; the reply is `tool_output`.
    #[serde(rename = "tool.call")]
    ToolCall {
        session: SessionId,
        name: String,
        #[serde(default)]
        args: serde_json::Value,
    },
}

fn yes() -> bool {
    true
}

fn default_cols() -> u16 {
    80
}

fn default_rows() -> u16 {
    24
}
