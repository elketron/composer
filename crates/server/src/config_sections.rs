//! The sections of `config.toml`. Every field has a default, so an empty file is valid.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// OpenAI-compatible base URL, including `/v1`.
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Sent as a bearer token when non-empty.
    #[serde(default)]
    pub api_key: String,
    /// Empty: use the first model from `GET /models`.
    #[serde(default)]
    pub model: String,
    /// Images in tool results (an MCP screenshot) go to the model as image parts of the tool
    /// message. Off for a server that takes images only from the user: the text names them.
    #[serde(default = "default_true")]
    pub tool_images: bool,
}

/// `[system1]`: a small fast model that decides action approvals (terminal input, background
/// processes, MCP and outside agents' tool calls, recipe runs) for the user: approve, reject, or
/// forward to the user. Trust changes (`folder:`, `mcp.toml`, policy pages) always go to the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct System1Config {
    /// Empty: off, every approval goes to the user.
    #[serde(default)]
    pub model: String,
    /// OpenAI-compatible base URL, including `/v1`. Empty: `[provider]`'s (sharing its slots, and
    /// its key unless `api_key` is set).
    #[serde(default)]
    pub base_url: String,
    /// Sent as a bearer token when non-empty.
    #[serde(default)]
    pub api_key: String,
    /// A decision that takes longer is forwarded to the user.
    #[serde(default = "default_system1_timeout")]
    pub timeout_secs: u64,
    /// Its `reasoning_effort` (none, low, medium, high): a decision needs little thought. Empty:
    /// not sent.
    #[serde(default = "default_low")]
    pub reasoning_effort: String,
}

fn default_low() -> String {
    "low".into()
}

fn default_system1_timeout() -> u64 {
    15
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Keep on localhost; expose to the tailnet with `tailscale serve`.
    #[serde(default = "default_bind")]
    pub bind: String,
    /// How this server introduces itself to apps (`GET /api/info`); empty: the hostname.
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachmentsConfig {
    /// Largest upload, in MB.
    #[serde(default = "default_attachment_max_mb")]
    pub max_mb: u64,
    /// Images are shrunk to this many pixels on their long side.
    #[serde(default = "default_image_max_edge")]
    pub image_max_edge: u32,
    /// Text (and PDF text) beyond this many KB is cut before it reaches the model.
    #[serde(default = "default_inline_max_kb")]
    pub inline_max_kb: usize,
}

/// `[terminals]`: the server-owned terminals of sessions (ticket 0176).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalsConfig {
    /// Output kept per terminal for a client that joins late, and for the agent to read.
    #[serde(default = "default_scrollback_kb")]
    pub scrollback_kb: usize,
}

fn default_scrollback_kb() -> usize {
    256
}

/// `[desktop]`: how this server is drawn in the desktop and the phone, so you can tell your
/// servers apart at a glance.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DesktopConfig {
    /// The server's colour, `#rrggbb`; empty: one derived from its name. Shown as the
    /// server's dot and title bar in the desktop and the phone.
    #[serde(default)]
    pub color: String,
}

/// `[viewer]`: wayai's live view, which composer serves to other devices at `/view`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewerConfig {
    /// The viewer page, e.g. `http://127.0.0.1:7878/view`; loopback hosts only. Empty: taken from
    /// `[mcp.wayai]`'s `url` (same address, path `/view`), and no viewer when that isn't set either.
    #[serde(default)]
    pub url: String,
    /// A file holding the viewer's bearer token (`~/` expanded): read, trimmed, for every request
    /// the proxy makes and sent as `Authorization: Bearer <token>`. Empty: `[mcp.wayai]`'s
    /// `bearer_file` when the viewer is at that server's address, else no header.
    #[serde(default)]
    pub bearer_file: String,
}

/// `[apps.<name>]`: one of the user's own local web servers, which composer serves to its clients
/// at `/apps/<name>/` (see [`crate::app_upstream`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    /// Where it listens, e.g. `http://127.0.0.1:4477`; loopback hosts only.
    pub url: String,
    /// What clients call it; empty: its name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Concurrent model requests across all sessions (the provider's parallelism).
    #[serde(default = "default_slots")]
    pub slots: u32,
    /// Tokens a turn may use before it stops: what the model writes plus what its steps bring
    /// into the context. The context it started with isn't counted. 0: no limit.
    #[serde(default = "default_turn_tokens")]
    pub turn_tokens: u32,
    /// Model calls per turn before it stops, on top of `turn_tokens`. 0 (the default): no limit.
    #[serde(default)]
    pub max_steps: u32,
    /// Prompt tokens at which compaction starts (milestone 5).
    #[serde(default = "default_compaction_threshold")]
    pub compaction_threshold: u32,
    /// A recipe run is killed after this many seconds.
    #[serde(default = "default_recipe_timeout")]
    pub recipe_timeout_secs: u64,
    /// `reasoning:` overrides it. Empty: not sent, the provider decides.
    #[serde(default)]
    pub reasoning_effort: String,
}

/// An MCP server: a `command` (stdio) or a `url` (streamable HTTP). `${VAR}` in `env` and
/// `headers` is taken from composer's environment.
///
/// `[mcp.<name>]` in `config.toml` is a server in every session. A site's own servers are in its
/// `mcp.toml` ([`crate::site_mcp`]) and end as this same shape once the user approved the file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct McpServerConfig {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// A file holding a bearer token for an HTTP server (`~/` expanded): read, trimmed, at every
    /// connect and sent as `Authorization: Bearer <token>`, unless `headers` sets `Authorization`
    /// itself. A file that cannot be read fails the connect.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bearer_file: String,
    /// Tools that run without asking: names or `prefix*` (`get_*`); `*` for all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
    /// Tools a workflow run also gets (it only reads), the same patterns as `allow`. Empty, the
    /// default: only agentic runs have the server's tools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_only: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Where a stdio server starts (`~/` and `${VAR}` expanded), instead of the home's
    /// sites folder.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cwd: String,
    /// Settings of an `.mcpb` bundle (`[mcp.<name>.config]`), by the manifest's `user_config` keys.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, String>,
    /// Why the server cannot start (a bundle's missing settings): `connect` fails with it.
    #[serde(skip)]
    pub problem: Option<String>,
}

/// llama.cpp's `llama-server` on this machine; set your own in config.toml.
fn default_base_url() -> String {
    "http://127.0.0.1:8080/v1".into()
}

fn default_attachment_max_mb() -> u64 {
    20
}

fn default_image_max_edge() -> u32 {
    1500
}

fn default_inline_max_kb() -> usize {
    100
}

fn default_true() -> bool {
    true
}

fn default_bind() -> String {
    "127.0.0.1:7420".into()
}

fn default_slots() -> u32 {
    2
}

fn default_turn_tokens() -> u32 {
    200_000
}

fn default_compaction_threshold() -> u32 {
    160_000
}

fn default_recipe_timeout() -> u64 {
    300
}
