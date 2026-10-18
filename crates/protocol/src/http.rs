//! HTTP request and response bodies (`/api/...`). All requests carry the device token.

use serde::{Deserialize, Serialize};

use crate::common::Url;

/// `GET /api/info`: what this server is (for the app's server list).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ServerInfo {
    /// `server.name` from config.toml, else the hostname.
    pub name: String,
    /// composer version.
    pub version: String,
    /// The model id requests go to.
    pub model: String,
    /// Model slots (the provider's parallelism).
    pub slots: u32,
}

/// `GET /api/sites`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SiteSummary {
    pub name: String,
    pub title: String,
    pub has_justfile: bool,
    pub has_ui: bool,
}

/// `GET /api/pages/{site}/{path}`. The `ETag` header carries `hash`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PageResponse {
    /// Canonical URL (the actual file).
    pub url: Url,
    pub title: String,
    /// Parsed frontmatter, so apps don't need a YAML parser.
    pub frontmatter: serde_json::Value,
    pub frontmatter_error: Option<String>,
    /// Full file contents, frontmatter included.
    pub raw: String,
    pub backlinks: Vec<Url>,
    /// Content hash; send back as `if_match` to detect concurrent edits.
    pub hash: String,
}

/// `PUT /api/pages/{site}/{path}`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PutPageRequest {
    pub content: String,
    /// Hash the client last saw. `None` overwrites unconditionally. Mismatch → 409.
    pub if_match: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PutPageResponse {
    pub url: Url,
    pub hash: String,
}

/// `GET /api/search?q=`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SearchHit {
    pub url: Url,
    pub title: String,
    pub site: String,
}

/// `POST /api/sessions/{id}/messages` — a micro app pushing a message to the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AppMessage {
    /// The app's site, shown as the message source.
    pub site: Option<String>,
    pub text: String,
    #[serde(default)]
    pub pages: Vec<Url>,
    #[serde(default)]
    pub images: Vec<Url>,
}

/// `GET /api/sessions/{id}/project/diff`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectDiff {
    pub path: String,
    pub branch: Option<String>,
    pub files: Vec<ChangedFile>,
    /// Unified diff of the working tree against HEAD.
    pub diff: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ChangedFile {
    pub path: String,
    /// Git status letter: `M`, `A`, `D`, `R`, `?`.
    pub status: String,
    pub added: u32,
    pub removed: u32,
}
