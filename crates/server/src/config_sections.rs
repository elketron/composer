//! The sections of `config.toml`. Every field has a default, so an empty file is valid.

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
    /// Where `new_project` creates projects (`~` is your home folder).
    #[serde(default = "default_projects_dir")]
    pub dir: String,
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
