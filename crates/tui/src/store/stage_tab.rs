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
    /// The changes of a site's folder.
    Changes {
        id: String,
        site: String,
        unseen: bool,
    },
    /// A server terminal: a shell, a program, a background process (`label`: its command).
    Terminal {
        id: String,
        terminal: String,
        label: Option<String>,
        unseen: bool,
    },
}

impl StageTab {
    /// What the tab is called, on the desk and in this list alike.
    pub fn id(&self) -> &str {
        match self {
            Self::Page { id, .. } | Self::Changes { id, .. } | Self::Terminal { id, .. } => id,
        }
    }

    /// What the tab bar says about it.
    pub fn title(&self) -> String {
        match self {
            Self::Page { url, .. } => url.strip_prefix("site://").unwrap_or(url).to_string(),
            Self::Changes { site, .. } => format!("changes · {site}"),
            Self::Terminal {
                label: Some(label), ..
            } => format!("$ {label}"),
            Self::Terminal { terminal, .. } => format!("term · {terminal}"),
        }
    }

    /// Opened by the agent and not looked at yet: a badge.
    pub fn unseen(&self) -> bool {
        match self {
            Self::Page { unseen, .. }
            | Self::Changes { unseen, .. }
            | Self::Terminal { unseen, .. } => *unseen,
        }
    }

    /// What to write back to the desk for it.
    pub fn desk_tab(&self) -> DeskTab {
        match self {
            Self::Page { url, .. } => DeskTab::Page { url: url.clone() },
            Self::Changes { site, .. } => DeskTab::Changes { site: site.clone() },
            Self::Terminal { terminal, .. } => DeskTab::Terminal {
                id: terminal.clone(),
            },
        }
    }
}
