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
    Flash(String, FlashLevel),

    // ---- chat & sessions ----
    SendMessage(String),
    CancelTurn,
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
    ReloadPage,

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
}
