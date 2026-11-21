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
pub struct ProjectsConfig {
    /// Where your projects live (`~` is your home folder): `new_project` creates them here,
    /// and a project given by name (`:project composer`, `--project composer`) is a folder here.
    #[serde(default = "default_projects_dir")]
    pub dir: String,
    /// Per-project settings, `[projects."~/Projects/composer"]` (or a name in `dir`).
    #[serde(flatten, default)]
    pub settings: BTreeMap<String, ProjectSettings>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectSettings {
    /// External harnesses tickets in this project may run on (`claude-code`). Default: none.
    #[serde(default)]
    pub external_harnesses: Vec<String>,
    /// Tools Claude Code may use here on top of `[claude_code] allowed_tools`, e.g. `Bash(cargo test:*)`.
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Which models its tickets may use: `"<harness>"` (any of its models) or
    /// `"<harness>:<model>"` (`"claude-code:sonnet"`, `"composer"`). Empty: any.
    #[serde(default)]
    pub models: Vec<String>,
    /// What it is called in the workspace and in app URLs; default: its folder's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tags apps can attach by (`when: ["tag:game"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Apps (sites) pinned to it, first in its rail, whatever their `when`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    /// Apps never attached to it, even when their `when` matches.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hide_apps: Vec<String>,
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

/// `[desktop]`: how a workspace is drawn in the desktop and the phone, so you can tell your
/// workspaces apart at a glance.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DesktopConfig {
    /// The workspace's colour, `#rrggbb`; empty: one derived from its name. Shown as the
    /// workspace's dot and title bar in the desktop and the phone.
    #[serde(default)]
    pub color: String,
}

/// `[viewer]`: wayai's live view, which composer serves to other devices at `<workspace>/view`.
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

/// An MCP server whose tools composer's agents may use, `[mcp.<name>]`: a `command` (stdio) or
/// a `url` (streamable HTTP). `${VAR}` in `env` and `headers` is taken from composer's environment.
///
/// An agent gets its tools from its own page's `tools:`, or from the site its session is in when
/// that site's `mcp:` names the server and `sites` here lists the site.
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
    /// The sites that may grant this server's tools with `mcp:` in their `index.md`: a site not
    /// listed here grants nothing, whatever its page says (agents can write pages; this file they
    /// cannot). Empty, the default: no site may.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sites: Vec<String>,
    /// Tools that run without asking: names or `prefix*` (`get_*`); `*` for all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Where a stdio server starts (`~/` and `${VAR}` expanded), instead of the workspace's
    /// sites.
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

fn default_projects_dir() -> String {
    "~/Projects".into()
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
