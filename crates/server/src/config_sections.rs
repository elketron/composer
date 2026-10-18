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
}

fn default_base_url() -> String {
    "https://ai.ts.elketron.com/v1".into()
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
