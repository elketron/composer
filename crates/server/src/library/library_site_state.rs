use crate::library::{LibraryCommit, SiteName};

/// A library site, and how it stands in this workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySiteState {
    pub site: SiteName,
    /// The last commit that changed it.
    pub commit: LibraryCommit,
    /// This workspace's sites installed from it, each with whether the library is ahead of it.
    pub installed: Vec<(SiteName, bool)>,
}

/// What an install did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub site: SiteName,
    pub from: SiteName,
    pub commit: LibraryCommit,
    pub files: usize,
    /// It has a `ui/`: built with its `ui-build` action (asked for like any recipe), not here.
    pub has_ui: bool,
}
