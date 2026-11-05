use composer_protocol::PageChangeKind;
use serde::{Deserialize, Serialize};

/// A session's interest in files: `pattern` is `site://x/**` (anything below), `site://x/*`
/// (one level) or one file's URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileWatch {
    pub session: String,
    pub pattern: String,
    /// Ends after the first change it reports ("tell me when it lands").
    #[serde(default)]
    pub once: bool,
}

/// A change that matched a watch, on its way to the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchHit {
    pub session: String,
    pub pattern: String,
    pub url: String,
    pub kind: PageChangeKind,
}
