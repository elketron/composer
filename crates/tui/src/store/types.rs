//! Plain data held by the stores.

use composer_protocol::ClientCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
    Hint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Chat,
    Page,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Pages,
    Sites,
    Sessions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PickerItem {
    pub title: String,
    pub detail: String,
    pub right: String,
    pub target: PickerTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PickerTarget {
    Url(String),
    Session(String),
}

/// One row-group in the chat.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatItem {
    User {
        text: String,
        source: Option<String>,
    },
    Thinking {
        text: String,
    },
    Text {
        text: String,
    },
    Tool {
        call_id: String,
        name: String,
        summary: String,
        result: Option<String>,
        is_error: bool,
    },
    System {
        text: String,
    },
}

/// The assistant message currently streaming in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Streaming {
    pub thinking: String,
    pub content: String,
}

/// A page as shown in the viewer.
#[derive(Debug, Clone, PartialEq)]
pub struct PageView {
    pub url: String,
    pub title: String,
    pub frontmatter: serde_json::Value,
    pub frontmatter_error: Option<String>,
    pub raw: String,
    pub backlinks: Vec<String>,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnState {
    Connecting,
    Connected,
    Disconnected(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Flash {
    pub text: String,
    pub level: FlashLevel,
    /// Hidden from the status bar after a few seconds.
    pub at: std::time::Instant,
}

/// What a sent command's reply means to us.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    ListSessions,
    CreateSession,
    Attach(String),
    Ignore,
}

/// A command waiting for its id to be assigned by the connection store.
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub command: ClientCommand,
    pub pending: Pending,
}

/// What to do once connected: attach a given session, create one, or resume the latest.
#[derive(Debug, Clone, Default)]
pub struct Startup {
    pub session: Option<String>,
    pub new: bool,
    pub agent: String,
    pub project: Option<String>,
}

/// A recipe waiting for your approval.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingApproval {
    pub request: String,
    pub session: String,
    pub site: String,
    pub recipe: String,
    pub args: Vec<String>,
    /// The recipe source (with dependencies) as `just --show` prints it.
    pub body: String,
    pub hash: String,
}
