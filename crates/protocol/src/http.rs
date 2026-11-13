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
    /// The workspace this server serves (`default` on a home from before workspaces). Older
    /// servers leave it out.
    #[serde(default)]
    pub workspace: String,
    /// The workspace's colour, `#rrggbb` (its `[desktop] color`, else from its name). Older
    /// servers leave it out.
    #[serde(default)]
    pub workspace_color: String,
    /// Where this workspace serves wayai's live view, as a path after the workspace's address
    /// (`/view`); absent when no viewer is configured. Needs the workspace's token, like the rest.
    /// Older servers leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer: Option<String>,
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
    /// `app:` of its `index.md`: the application (`[applications.<name>]`) it starts. A site with
    /// `app` or `mcp` set is a workflow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// `mcp:` of its `index.md`: the MCP servers whose tools the agent has while in the site.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
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
    /// A legacy worker page (`role: worker`); current workers are the default agent.
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
    /// Its project page (`site://projects/<name>`), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
}

/// `project.overview`: what is going on in one project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectOverview {
    pub name: String,
    /// Its project page (`site://projects/<name>`), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// `~/Projects/<name>`.
    pub path: String,
    /// The absolute folder, as sessions' `project` holds it.
    pub folder: String,
    /// Checked-out branch; `None` outside git or with a detached HEAD.
    pub branch: Option<String>,
    /// Files changed and not committed (git).
    pub uncommitted: u32,
    /// The sites operating on it, each with its app tile.
    #[serde(default)]
    pub sites: Vec<ProjectSiteSummary>,
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

/// One site operating on a project, as the project hub shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectSiteSummary {
    pub site: String,
    pub title: String,
    /// Its tile, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<AppInfo>,
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
    /// A project-local app (`<project>/.composer/apps/<site>`); `why` says whether it is allowed.
    #[serde(default)]
    pub local: bool,
    /// A client of the workspace (`[apps] clients`): opened with its full access.
    #[serde(default)]
    pub client: bool,
    /// Built with an older sdk: it shows nothing until rebuilt (`ui-build`, or `composer app
    /// rebuild`).
    #[serde(default)]
    pub outdated: bool,
    /// On Home, for a `tile` app: its tile, the body of `site://<site>/tile.md` (which the app or
    /// the agent keeps current).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile_text: Option<String>,
}

/// `GET <app>/api/project`: the project an app was opened on (the sdk's `project()`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectInfo {
    /// Its name in the workspace (and in the app's URL).
    pub name: String,
    /// Its project page (`site://projects/<name>`), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// The absolute folder.
    pub root: String,
    /// The branch checked out, for a git repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The sites operating on it (the project page's `sites:`).
    #[serde(default)]
    pub sites: Vec<String>,
    /// `[projects."<path>"] tags`.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// `POST /api/projects/<name>/apps/<site>/open`: where to load the app (`<base>/a/<token>/`: its
/// token is in the path, and its API is under it), and the token itself for a client that calls
/// the app's API directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AppOpened {
    pub url: String,
    /// The app's own token; `None` for a client app (its URL has a one-time code instead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// The project it was opened on, by its name; `None` on Home.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// `POST /api/web-session`: a new browser session of the caller's kind, for a client that sets
/// the workspace's cookie itself (the desktop's panes, the phone's webviews) instead of putting a
/// token in a cookie or a URL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WebSession {
    /// The cookie's name.
    pub cookie_name: String,
    /// The session secret (`ses_...`): the cookie's value.
    pub value: String,
    /// The cookie's `Path` (the workspace's routes).
    pub path: String,
}

/// A project open in the workspace (`projects.open`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OpenProject {
    pub name: String,
    /// The clients (TUI, desktop, app sockets) that have it open.
    pub clients: u32,
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
