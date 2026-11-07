use std::fmt;

use crate::apps::AppCondition;

/// Why an app is in a list: pinned to the project, matched by one of its `when` entries, or on
/// the workspace's Home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachReason {
    Pinned,
    Matched(AppCondition),
    Home,
    /// A project-local app the user allowed.
    Local,
    /// A project-local app not allowed (yet, or since it changed).
    LocalNotAllowed,
}

impl fmt::Display for AttachReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pinned => f.write_str("pinned"),
            Self::Matched(condition) => write!(f, "matched {condition}"),
            Self::Home => f.write_str("home"),
            Self::Local => f.write_str("local"),
            Self::LocalNotAllowed => f.write_str("local, not allowed"),
        }
    }
}
