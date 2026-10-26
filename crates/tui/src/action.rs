//! Everything that changes state is an `Action`.

use composer_protocol::http::{SearchHit, SiteSummary};
use composer_protocol::{ApprovalDecision, Attachment, ServerMessage};

use crate::store::types::{FlashLevel, Mode, PageView, Pane, PickerKind};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // ---- ui ----
    Quit,
    SetMode(Mode),
    FocusPane(Pane),
    FocusNext,
    ToggleMaximize,
    ToggleThinking,
    ToggleToolDetails,
    ToggleHelp,
    Flash(String, FlashLevel),

    // ---- chat & sessions ----
    SendMessage(String),
    /// Attach a file on this machine (a pasted path, `:file <path>`).
    AttachPath(String),
    /// Attach the image on the clipboard (`space a`).
    AttachClipboard,
    /// Text selected with the mouse, to the clipboard.
    CopyText(String),
    /// The clipboard image, read off the UI thread: (name, PNG bytes).
    ClipboardRead(Result<(String, Vec<u8>), String>),
    FileUploaded {
        id: u64,
        result: Result<Attachment, String>,
    },
    /// Remove the last attached file (backspace in an empty message).
    DropAttachment,
    /// Remove every attached file (`:file clear`).
    ClearAttachments,
    CancelTurn,
    /// Save memory and summarize older messages now.
    Compact,
    NewSession {
        agent: Option<String>,
    },
    AttachSession(String),
    /// Scrolled up to the oldest message shown: fetch the ones before it.
    LoadOlder,
    /// Back to the previous session (worker → orchestrator).
    JumpBack,
    /// The next (1) or previous (-1) session of this orchestration: orchestrator, workers.
    CycleOrchestration(i32),
    SetProject(Option<String>),
    /// `:model <id>`: the model the session's turns use; `None`: the server's.
    SetModel(Option<String>),
    /// `:model`: say which model the session uses.
    ShowModel,
    /// Start a chat in this project (a folder).
    NewSessionIn(String),
    /// Show a project's overview (a name or folder; `None`: the current session's project).
    ProjectView(Option<String>),
    /// `gp`: the project view on or off.
    ToggleProjectView,
    /// `:mcp`: the MCP servers, their state and tools.
    McpServers,
    /// `:reload`: the server reads config.toml again.
    ReloadConfig,
    /// `:pipeline`: the pipelines and the templates new ones come from.
    Pipelines,
    /// `:pipeline new <kind> [name]`: a pipeline site from a template.
    PipelineNew {
        kind: String,
        name: Option<String>,
    },
    /// `:work new <pipeline> <name> [project]`: a piece of work in a pipeline (default project:
    /// the project view's, else this session's).
    WorkNew {
        pipeline: String,
        name: String,
        project: Option<String>,
    },
    /// `:work run [work] [stage]`: run a work's next stage (or `stage`); default work: the one
    /// the open page is in.
    WorkRun {
        work: Option<String>,
        stage: Option<String>,
    },
    /// Narrow the session picker to a project (a name or folder); `None`: all.
    Scope(Option<String>),
    /// The project list for the picker, for `query`.
    ProjectsLoaded {
        query: String,
        projects: Vec<composer_protocol::http::ProjectSummary>,
    },
    /// Show the project's external harnesses, or turn one (`claude-code[:model]`) on or off.
    Harness {
        on: Option<bool>,
        entry: String,
    },

    // ---- pages ----
    OpenUrl(String),
    PageBack,
    PageForward,
    ToggleFollow,
    EditPage,
    /// Open a site's app in the browser: the given site, or the current page's.
    OpenApp(Option<String>),
    ReloadPage,

    // ---- changes view (active project) ----
    ToggleChanges,
    RefreshChanges,
    DiffLoaded(composer_protocol::http::ProjectDiff),
    DiffFailed(String),
    /// Merge a ticket's branch (the current worker session's ticket when `None`).
    TicketMerge(Option<String>),
    /// Delete a ticket's worktree and branch.
    TicketDiscard(String),
    /// Open a project file in the editor at a line.
    OpenFileAt {
        path: String,
        line: u32,
    },

    // ---- approvals ----
    ApprovalDecide(ApprovalDecision),
    /// Read the recipe in the editor before deciding.
    ApprovalView,

    // ---- picker ----
    OpenPicker(PickerKind),
    PickerQuery(String),
    PickerMove(i32),
    PickerAccept,
    ClosePicker,

    // ---- results from the outside ----
    Connected,
    Disconnected(String),
    Server(ServerMessage),
    PageLoaded(PageView),
    PageFailed {
        url: String,
        error: String,
    },
    SearchResults {
        query: String,
        hits: Vec<SearchHit>,
    },
    SitesLoaded(Vec<SiteSummary>),
    /// Sites and pages for `:` completion.
    CatalogLoaded {
        sites: Vec<SiteSummary>,
        pages: Vec<SearchHit>,
    },
    /// The sites, fetched before opening `site`'s app at `url`; `None` if that failed.
    AppChecked {
        site: String,
        url: String,
        sites: Option<Vec<SiteSummary>>,
    },
}

/// Result of feeding a key to the keymap. Short-lived, so its size does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum KeyOutcome {
    Action(Action),
    /// A prefix of a binding (`Space`, `ctrl-w`): wait for the next key.
    Pending,
    /// Not bound: the focused component may handle it.
    Unbound,
    /// A started sequence that matched nothing: these keys go to the component.
    Replay(Vec<crossterm::event::KeyEvent>),
}
