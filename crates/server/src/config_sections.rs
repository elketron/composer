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

/// A fast model that decides workers' approvals instead of their orchestrator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct System1Config {
    /// Empty: off (the orchestrator decides).
    #[serde(default)]
    pub model: String,
    /// Empty: the provider's (and its key, unless `api_key` is set).
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    /// Its `reasoning_effort`: a decision needs little thought. Empty: not sent.
    #[serde(default = "default_low")]
    pub reasoning_effort: String,
}

fn default_low() -> String {
    "low".into()
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

/// `[sandbox]`: the world each spawn an agent or a site controls runs in (`bwrap`). The home's
/// global config sets the machine's paths; a workspace's config may add `binds_ro`, `binds_rw` and
/// `devices` of its own. Paths are `~/` or absolute; missing ones are skipped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// `false` runs spawns as they are, without a sandbox: only by the user's explicit choice.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// System folders mounted read-only besides `/usr` and `/etc`.
    #[serde(default = "default_sandbox_system_dirs")]
    pub system_dirs: Vec<String>,
    /// Toolchains, read-only.
    #[serde(default = "default_sandbox_toolchains")]
    pub toolchains: Vec<String>,
    /// Caches package managers write to, read-write (shared by every workspace).
    #[serde(default = "default_sandbox_caches")]
    pub caches: Vec<String>,
    /// More folders or files, read-only.
    #[serde(default)]
    pub binds_ro: Vec<String>,
    /// More folders or files, read-write (`~/Projects` for a site whose recipes take any project).
    /// Another workspace's projects inside one stay hidden; a bind into the composer home or
    /// another workspace's project is refused.
    #[serde(default)]
    pub binds_rw: Vec<String>,
    /// Devices (`/dev/dri` for the GPU).
    #[serde(default)]
    pub devices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaudeCodeConfig {
    /// The Claude Code CLI. It must already be logged in (`claude auth login`).
    #[serde(default = "default_claude_command")]
    pub command: String,
    /// `--model` (`sonnet`, `opus`, a full name); empty: the CLI's default.
    #[serde(default)]
    pub model: String,
    /// `--max-turns` per run.
    #[serde(default = "default_claude_max_turns")]
    pub max_turns: u32,
    /// A run is stopped after this many minutes.
    #[serde(default = "default_claude_timeout")]
    pub timeout_minutes: u64,
    /// `--allowedTools`: what it may do without asking (anything else is denied).
    #[serde(default = "default_claude_tools")]
    pub allowed_tools: Vec<String>,
    /// Anything outside `allowed_tools` is asked of the user (an approval in the worker's
    /// session); `false` denies it without asking.
    #[serde(default = "default_true")]
    pub ask: bool,
    /// Folders it may read (Read, Glob, Grep) but never edit: dependency sources and model
    /// caches. `~/` or absolute; the ones that don't exist are skipped.
    #[serde(default = "default_claude_read_dirs")]
    pub read_dirs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemindersConfig {
    /// When a reminder with a `due` date but no `time` goes off (local time, `HH:MM`).
    #[serde(default = "default_reminder_time")]
    pub default_time: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelinesConfig {
    /// Build a new pipeline's board app right away (its `ui-build` action, needs `just` and `bun`).
    #[serde(default = "default_true")]
    pub build_board: bool,
    /// After how many works with your edits to the same stage an agent proposes changing the
    /// stage's contract or `_config` (0: never).
    #[serde(default = "default_learn_after")]
    pub learn_after: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Concurrent model requests across all sessions (the provider's parallelism).
    #[serde(default = "default_slots")]
    pub slots: u32,
    /// Tokens a turn may use before it stops (a worker's orchestrator then decides whether it
    /// goes on): what the model writes plus what its steps bring into the context. The context
    /// it started with isn't counted. 0: no limit.
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
    /// Worker processes running at once (each needs a model slot to make progress).
    #[serde(default = "default_max_workers")]
    pub max_workers: u32,
    /// Memory a worker run (and all it starts) may use, in systemd's syntax: `50%` of RAM,
    /// `12G`; empty: no cap. Needs systemd (each run gets its own scope).
    #[serde(default = "default_worker_memory_max")]
    pub worker_memory_max: String,
    /// How much models think (none, low, medium, high), sent as `reasoning_effort`; an agent's
    /// `reasoning:` overrides it. Empty: not sent, the provider decides.
    #[serde(default)]
    pub reasoning_effort: String,
}

/// An MCP server whose tools composer's agents may use, `[mcp.<name>]`: a `command` (stdio) or
/// a `url` (streamable HTTP). `${VAR}` in `env` and `headers` is taken from composer's environment.
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
    /// What a stdio server needs from outside its workspace's sandbox, read-only and read-write
    /// (`~/` and `${VAR}` expanded; checked like `[sandbox]` binds).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binds_ro: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binds_rw: Vec<String>,
    /// Tools that run without asking: names or `prefix*` (`get_*`); `*` for all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// An app a site's app can launch on this machine's desktop: one `[[launcher]]` entry per
/// launch button; declaring it here is its approval (like an allowed folder).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LauncherEntry {
    /// What the apps call it by (`POST /api/launcher/<name>`).
    pub name: String,
    /// The program to run: a name on `PATH` or a path.
    pub command: String,
    /// The folder to open it on: a project in the projects directory, or a folder inside one
    /// (a scoped slice). It is passed to the command as its last argument, after `args`.
    pub folder: String,
    /// Extra arguments, put before the folder path.
    #[serde(default)]
    pub args: Vec<String>,
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

fn default_claude_command() -> String {
    "claude".into()
}

fn default_claude_max_turns() -> u32 {
    60
}

fn default_claude_timeout() -> u64 {
    60
}

/// Where package managers keep sources a model can read. Only the source folders: `~/.cargo`
/// itself can hold `credentials.toml`, `~/.cache/huggingface` a `token`.
fn default_claude_read_dirs() -> Vec<String> {
    [
        "~/.cargo/registry/src",
        "~/.cargo/git/checkouts",
        "~/.rustup/toolchains",
        "~/.cache/huggingface/hub",
        "~/go/pkg/mod",
        "~/.pub-cache/hosted",
        "~/.pub-cache/git",
        "~/.nuget/packages",
        "~/.bun/install/cache",
        "~/.cache/uv",
        "~/.gradle/caches/modules-2/files-2.1",
        "~/.m2/repository",
    ]
    .map(String::from)
    .to_vec()
}

fn default_claude_tools() -> Vec<String> {
    [
        "Read",
        "Edit",
        "Write",
        "Glob",
        "Grep",
        "mcp__composer",
        "Bash(git status:*)",
        "Bash(git diff:*)",
        "Bash(git log:*)",
        "Bash(git add:*)",
        "Bash(git commit:*)",
    ]
    .map(String::from)
    .to_vec()
}

fn default_sandbox_system_dirs() -> Vec<String> {
    ["/opt", "/sys", "/run/systemd/resolve"]
        .map(String::from)
        .to_vec()
}

fn default_sandbox_toolchains() -> Vec<String> {
    ["~/.rustup", "~/.cargo/bin", "~/.bun/bin", "~/.local/bin"]
        .map(String::from)
        .to_vec()
}

fn default_sandbox_caches() -> Vec<String> {
    [
        "~/.cargo/registry",
        "~/.cargo/git",
        "~/.bun/install/cache",
        "~/.cache/uv",
    ]
    .map(String::from)
    .to_vec()
}

fn default_true() -> bool {
    true
}

fn default_reminder_time() -> String {
    "09:00".into()
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

fn default_max_workers() -> u32 {
    1
}

fn default_worker_memory_max() -> String {
    "50%".into()
}

fn default_learn_after() -> u32 {
    3
}
