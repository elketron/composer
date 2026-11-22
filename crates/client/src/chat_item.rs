//! Plain data of a conversation as a client shows it.

use composer_protocol::ApprovalKind;

/// One row-group in the chat.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatItem {
    User {
        text: String,
        /// Who sent it when not you: `3d app`, `worker #a1b2`, `reminder`, `watch`.
        source: Option<String>,
        /// Names of the files attached to it.
        files: Vec<String>,
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
        /// The interesting argument on one line.
        summary: String,
        /// What it changes (an edit's diff, a to-do list).
        detail: Option<String>,
        /// Made inside another call (a Claude Code sub-agent's).
        nested: bool,
        result: Option<String>,
        is_error: bool,
    },
    System {
        text: String,
    },
    /// A decision of system-1 (the `[system1]` model) on an approval: `system-1 approved …`.
    System1 {
        text: String,
    },
    /// Older messages were replaced by a summary.
    Compacted {
        summary: String,
        messages: u32,
    },
}

/// The assistant message streaming in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Streaming {
    pub thinking: String,
    pub content: String,
}

/// Something waiting for the user's yes (`approval.request`), until it is answered.
#[derive(Debug, Clone, PartialEq)]
pub struct Approval {
    pub request: String,
    pub session: String,
    pub kind: ApprovalKind,
    /// One line saying what is asked (empty for recipes: show site and recipe).
    pub title: String,
    pub site: String,
    pub recipe: String,
    pub args: Vec<String>,
    /// What to look at: the recipe, the changes, the tool's input.
    pub body: String,
    /// A tool's rule for `always`.
    pub rule: Option<String>,
}
