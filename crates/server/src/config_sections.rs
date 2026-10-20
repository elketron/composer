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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemindersConfig {
    /// When a reminder with a `due` date but no `time` goes off (local time, `HH:MM`).
    #[serde(default = "default_reminder_time")]
    pub default_time: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Concurrent model requests across all sessions (the provider's parallelism).
    #[serde(default = "default_slots")]
    pub slots: u32,
    /// Model calls per turn before the turn is stopped.
    #[serde(default = "default_max_steps")]
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

fn default_reminder_time() -> String {
    "09:00".into()
}

fn default_bind() -> String {
    "127.0.0.1:7420".into()
}

fn default_slots() -> u32 {
    2
}

fn default_max_steps() -> u32 {
    40
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
