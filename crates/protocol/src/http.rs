//! HTTP request and response bodies (`/api/...`). All requests carry the device token.

use serde::{Deserialize, Serialize};

use crate::common::{HarnessSwitch, Url};

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
    /// Where projects live (`[projects] dir`), `~` for the home folder. Older servers leave it out.
    #[serde(default)]
    pub projects_dir: String,
}

/// `GET /api/sites`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SiteSummary {
    pub name: String,
    pub title: String,
    pub has_justfile: bool,
    /// Has a `ui/` folder: an app, built or not.
    pub has_ui: bool,
    /// The app is built (`ui/dist/index.html`) and can be opened. Older servers leave it out.
    #[serde(default)]
    pub app_built: bool,
    /// One line about the site: its index's `description:`, else its first sentence.
    #[serde(default)]
    pub description: String,
    /// Pages in the site.
    #[serde(default)]
    pub pages: u32,
}

/// `GET /api/agents`: the agent definitions (`site://agents`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AgentSummary {
    pub name: String,
    pub title: String,
    /// `description:`, else the title.
    pub description: String,
    /// A worker agent (`role: worker`): runs tickets, not chats.
    pub worker: bool,
}

/// `GET /api/projects`: the folders in the projects directory, most recently changed first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectSummary {
    /// Folder name; what `project.set` and `session.create` accept.
    pub name: String,
    /// `~/Projects/<name>`.
    pub path: String,
    /// RFC 3339.
    pub modified: String,
    /// The absolute folder, as sessions' `project` holds it.
    #[serde(default)]
    pub folder: String,
    /// Chats and orchestrators in it (workers count with their orchestrator).
    #[serde(default)]
    pub sessions: u32,
    /// Of those, the ones busy now (or with a worker busy).
    #[serde(default)]
    pub running: u32,
    /// Approvals waiting in its sessions.
    #[serde(default)]
    pub waiting: u32,
    /// When one of its sessions last did something (RFC 3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
}

/// `project.overview`: what is going on in one project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectOverview {
    pub name: String,
    /// `~/Projects/<name>`.
    pub path: String,
    /// The absolute folder, as sessions' `project` holds it.
    pub folder: String,
    /// Checked-out branch; `None` outside git or with a detached HEAD.
    pub branch: Option<String>,
    /// Files changed and not committed (git).
    pub uncommitted: u32,
    /// Works of any pipeline that link it.
    pub works: Vec<WorkSummary>,
    /// Tickets whose worktree of it is still there: their branch waits to be merged or discarded.
    pub branches: Vec<TicketBranchSummary>,
    /// Its memory page (`site://memory/projects/<name>`), when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<PageExcerpt>,
    /// External harnesses config.toml allows here, and whether each is on.
    pub harnesses: Vec<HarnessSwitch>,
    /// Tool rules remembered for it (Claude Code's `always`).
    pub tool_rules: u32,
}

/// A piece of work in a pipeline (`site://<pipeline>/<work>/index.md`, `type: work`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorkSummary {
    pub url: Url,
    /// The pipeline's site.
    pub pipeline: String,
    pub title: String,
    /// Its `status:` (the stage it is at, `done`, …).
    pub status: Option<String>,
}

/// A ticket's branch that is still there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TicketBranchSummary {
    pub ticket: String,
    pub title: String,
    /// `ticket/<id>`.
    pub branch: String,
    /// The ticket's `status:`.
    pub status: Option<String>,
    /// Commits on it the project's branch doesn't have.
    pub commits: u32,
}

/// The start of a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PageExcerpt {
    pub url: Url,
    pub excerpt: String,
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
    /// Unified diff of the working tree against HEAD (a ticket's worktree: against where its
    /// branch left the project's branch).
    pub diff: String,
    /// Set for a ticket's worktree: what merging it would do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketReview>,
}

/// A ticket branch against the branch it would be merged into.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TicketReview {
    pub ticket: String,
    /// `ticket/<id>`.
    pub branch: String,
    /// The branch checked out in the project; `None` when its HEAD is detached.
    pub target: Option<String>,
    /// Commits on the ticket branch that the target doesn't have (counting uncommitted changes
    /// as one).
    pub ahead: u32,
    /// Those commits, `<short hash> <subject>`, newest first.
    #[serde(default)]
    pub commits: Vec<String>,
    /// The worktree has changes nobody committed (merging commits them first).
    pub uncommitted: bool,
    /// Files that would conflict; empty when it merges cleanly.
    pub conflicts: Vec<String>,
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
