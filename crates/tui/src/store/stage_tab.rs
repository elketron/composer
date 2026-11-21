//! A tab the stage can draw (ticket 0208): the desk's tabs a terminal can show.

use composer_protocol::DeskTab;

/// A tab the stage can draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageTab {
    /// A page (`site://…`).
    Page {
        id: String,
        url: String,
        unseen: bool,
    },
    /// A project's changes.
    Changes {
        id: String,
        site: String,
        unseen: bool,
    },
}

impl StageTab {
    /// What the tab is called, on the desk and in this list alike.
    pub fn id(&self) -> &str {
        match self {
            Self::Page { id, .. } | Self::Changes { id, .. } => id,
        }
    }

    /// What the tab bar says about it.
    pub fn title(&self) -> String {
        match self {
            Self::Page { url, .. } => url.strip_prefix("site://").unwrap_or(url).to_string(),
            Self::Changes { site, .. } => format!("changes · {site}"),
        }
    }

    /// Opened by the agent and not looked at yet: a badge.
    pub fn unseen(&self) -> bool {
        match self {
            Self::Page { unseen, .. } | Self::Changes { unseen, .. } => *unseen,
        }
    }

    /// What to write back to the desk for it.
    pub fn desk_tab(&self) -> DeskTab {
        match self {
            Self::Page { url, .. } => DeskTab::Page { url: url.clone() },
            Self::Changes { site, .. } => DeskTab::Changes { site: site.clone() },
        }
    }
}
