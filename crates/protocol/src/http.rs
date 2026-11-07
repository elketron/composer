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
    /// Where it is: the stage it is at and its state (`01 spec · waiting`), or `done`.
    pub status: Option<String>,
    /// Its stages in order, with their state.
    #[serde(default)]
    pub stages: Vec<StageState>,
    /// The project it links (`~/Projects/game`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// A work's stage and where it is: `waiting`, `running`, `review`, `approved`, `rerun`,
/// `stopped`, `done` (its `_stage.md`'s `status:`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StageState {
    /// Its folder in the work (`02_script`).
    pub name: String,
    pub status: String,
    /// The ticket of its last run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    /// What its last run said it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The pages it wrote (in its folder).
    #[serde(default)]
    pub outputs: Vec<Url>,
    /// It ran, and something it read changed since (or it comes after a stage that is stale).
    #[serde(default)]
    pub stale: bool,
    /// Its inputs that changed since it ran.
    #[serde(default)]
    pub changed: Vec<Url>,
}

/// An app (a site with a `ui/` or an `app:` block) in a project's rail or on the workspace's Home:
/// `GET /api/apps`, `GET /api/projects/<name>/apps`, ws `apps.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AppInfo {
    pub site: String,
    /// The rail label, else the site's title.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// `project`, `global` or `both`.
    pub scope: String,
    /// `pinned`, `matched files:*.FCStd`, `home`.
    pub why: String,
    /// It contributes a tile to Home.
    #[serde(default)]
    pub tile: bool,
    /// File patterns it opens.
    #[serde(default)]
    pub opens: Vec<String>,
    /// `ui/dist/index.html` exists.
    #[serde(default)]
    pub built: bool,
}

/// A project's apps, or Home's; `errors` are sites whose `app:` block doesn't parse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AppList {
    /// The project's name; `None` for Home.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub apps: Vec<AppInfo>,
    #[serde(default)]
    pub errors: Vec<String>,
}

/// An MCP server from `config.toml` (`[mcp.<name>]`), whose tools agents may list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct McpServerInfo {
    pub name: String,
    /// `stdio` or `http`.
    pub transport: String,
    /// `ready`, `failed` (see `error`) or `off` (`enabled = false`).
    pub state: String,
    pub error: Option<String>,
    /// Its tools as agents list them (`github__get_issue`).
    pub tools: Vec<String>,
    /// The agents whose `tools:` include some of them.
    pub agents: Vec<String>,
}

/// A pipeline: a site of staged work (`type: pipeline`), or a template one is made from
/// (`templates/pipelines/<kind>/`: `site` is the kind).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PipelineSummary {
    pub site: String,
    pub title: String,
    /// One line: what kind of work it is for.
    pub about: String,
    /// Its stage contracts in order.
    pub stages: Vec<StageSummary>,
    /// Whether a work must link a project (coding: where the code lives).
    pub needs_project: bool,
    /// How many works it holds.
    pub works: u32,
}

/// A stage contract (`site://<pipeline>/stages/02_script.md`, `type: stage`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StageSummary {
    /// `02_script`: its file in `stages/`, and its folder in each work.
    pub name: String,
    pub title: String,
    /// Waits for the human after it.
    pub gate: bool,
    /// `composer` or an external one (`claude-code`); `None` is the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Writes code in the linked project, in a ticket worktree.
    pub worktree: bool,
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
    /// The structural diff of this file from `difft` (plain text, line numbers in its gutter);
    /// absent when difft isn't installed, doesn't know the language, failed or timed out: the
    /// text `diff` is the fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic: Option<String>,
}

/// `GET /api/actions/{site}`: one public recipe in a site's action registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ActionSummary {
    pub name: String,
    pub doc: Option<String>,
    /// `name`, `name=default` or `*name`, as in `actions`.
    pub parameters: Vec<String>,
    /// Whether this version may run unasked; otherwise the caller's session is asked.
    pub approved: bool,
}

/// `POST /api/actions/{site}/{recipe}`: run one public recipe of a site's justfile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RunActionRequest {
    /// Recipe arguments, in order.
    #[serde(default)]
    pub args: Vec<String>,
    /// The session whose clients are asked when the recipe needs approval (an app's URL carries
    /// `session=<id>`). Without one, a recipe that needs approval is not run.
    #[serde(default)]
    pub session: Option<String>,
}

/// The answer to `POST /api/actions/{site}/{recipe}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RunActionResponse {
    /// `ok`, `failed`, `timed out` or `killed`.
    pub status: String,
    pub exit_code: Option<i32>,
    pub output: String,
}

/// A review comment on a changed line, appended to the ticket page as
/// `- file:line — message`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DiffComment {
    pub file: String,
    /// The first and last line the comment is about, in the new file (`start == end` for one
    /// line; a removed line anchors where it sits in the new file).
    pub start: u32,
    pub end: u32,
    pub message: String,
    /// The commented line's text; written into the comment when it fits one line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
}

/// `library.list`: one site of the library, and this workspace's copies of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LibrarySiteInfo {
    pub site: String,
    /// The last library commit that changed it (12 digits).
    pub commit: String,
    /// This workspace's sites installed from it.
    pub installed: Vec<InstalledSiteInfo>,
}

/// A workspace site installed from the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct InstalledSiteInfo {
    pub site: String,
    /// The library changed it since: `library.update` brings it in.
    pub behind: bool,
}

/// `library.update`: what happened to one file of an installed site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LibraryChange {
    /// `workspaces/<ws>/sites/<site>/<file>`.
    pub path: String,
    /// `added`, `replaced`, `removed`, `merged`, `conflict` or `review`.
    pub outcome: String,
    /// For `conflict` and `review`: the library's version (with conflict markers) to look at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,
}

/// `library.publish`'s plan: what publishing a workspace site to the library would do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LibraryPublishPlan {
    /// The workspace's site.
    pub site: String,
    /// Its name in the library.
    pub name: String,
    /// The library has no such site yet.
    pub new_site: bool,
    /// The files that differ from the library (`outcome`: `added`, `changed`, `removed`).
    pub changes: Vec<LibraryChange>,
    /// Top-level entries of the site that are not published (works, tickets, pages).
    pub left_out: Vec<String>,
    /// The whole plan as text, diff included: what to show before asking.
    pub text: String,
    /// Pass back as `approved` to publish exactly this.
    pub digest: String,
}
