//! A tab the stage can draw (ticket 0208): the desk's tabs a terminal can show, plus the
//! project overview, which is this terminal's own.

use composer_protocol::DeskTab;

use crate::store::desk_store::PROJECT_TAB;

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
        project: String,
        unseen: bool,
    },
    /// A project's overview: this terminal's own tab, never on the desk.
    Project { project: Option<String> },
}

impl StageTab {
    /// What the tab is called, on the desk and in this list alike.
    pub fn id(&self) -> &str {
        match self {
            Self::Page { id, .. } | Self::Changes { id, .. } => id,
            Self::Project { .. } => PROJECT_TAB,
        }
    }

    /// What the tab bar says about it.
    pub fn title(&self) -> String {
        match self {
            Self::Page { url, .. } => url.strip_prefix("site://").unwrap_or(url).to_string(),
            Self::Changes { project, .. } => format!("changes · {project}"),
            Self::Project {
                project: Some(project),
            } => format!(
                "project · {}",
                project
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or(project)
            ),
            Self::Project { project: None } => "project".to_string(),
        }
    }

    /// Opened by the agent and not looked at yet: a badge.
    pub fn unseen(&self) -> bool {
        match self {
            Self::Page { unseen, .. } | Self::Changes { unseen, .. } => *unseen,
            Self::Project { .. } => false,
        }
    }

    /// What to write back to the desk for it; `None` for a tab that lives only here.
    pub fn desk_tab(&self) -> Option<DeskTab> {
        match self {
            Self::Page { url, .. } => Some(DeskTab::Page { url: url.clone() }),
            Self::Changes { project, .. } => Some(DeskTab::Changes {
                project: project.clone(),
            }),
            Self::Project { .. } => None,
        }
    }
}
