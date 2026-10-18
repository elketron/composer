//! Everything that changes state is an `Action`.

use composer_protocol::http::{SearchHit, SiteSummary};
use composer_protocol::{ApprovalDecision, ServerMessage};

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
    CancelTurn,
    /// Save memory and summarize older messages now.
    Compact,
    NewSession {
        agent: Option<String>,
    },
    AttachSession(String),
    /// Back to the previous session (worker → orchestrator).
    JumpBack,
    SetProject(Option<String>),

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
