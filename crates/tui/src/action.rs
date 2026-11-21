//! Everything that changes state is an `Action`.

use composer_protocol::http::{SearchHit, SiteSummary};
use composer_protocol::{ApprovalDecision, Attachment, RunKind, ServerMessage};

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
    /// Text to the clipboard: a mouse selection.
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
    /// `:new [kind]`: a new session of that run kind (default: the startup's).
    NewSession {
        kind: Option<RunKind>,
    },
    AttachSession(String),
    /// Scrolled up to the oldest message shown: fetch the ones before it.
    LoadOlder,
    /// Back to the previous session.
    JumpBack,
    /// `:model <id>`: the model the session's turns use; `None`: the server's.
    SetModel(Option<String>),
    /// `:model`: say which model the session uses.
    ShowModel,
    /// `:mcp`: the MCP servers, their state and tools.
    McpServers,
    /// `:reload`: the server reads config.toml again.
    ReloadConfig,
    /// Narrow the session picker to the sessions in a site; `None`: all.
    Scope(Option<String>),

    // ---- stage tabs (the session's desk) ----
    /// Show the next (1) or previous (-1) tab the stage can draw.
    CycleTab(i32),
    /// Show the tab at `index` of the stage's tab list (a click on the tab bar).
    ShowTab(usize),
    /// Close the tab on the screen; its neighbour is shown, as on the desk.
    CloseTab,

    // ---- pages ----
    OpenUrl(String),
    PageBack,
    PageForward,
    ToggleFollow,
    EditPage,
    /// Enter a site for the shown session, or (`None`) leave its site.
    SetSite(Option<String>),
    ReloadPage,

    // ---- changes view (the folder of the session's site) ----
    ToggleChanges,
    RefreshChanges,
    DiffLoaded(composer_protocol::http::ProjectDiff),
    DiffFailed(String),
    /// Semantic (difft) diff or the text diff in the changes view.
    DiffSemantic,
    /// Open a file of the folder in the editor at a line.
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
