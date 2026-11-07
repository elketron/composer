use std::path::PathBuf;

use crate::apps::GrantedProject;
use crate::workspace_home::WorkspaceHome;

/// What an app token lets an app do: it is `site`'s app, opened on `project` (or on the
/// workspace's Home: `None`). It reads the workspace's web; it writes only its own site and its
/// project, and runs only its own site's actions (decision 2026-11-07).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppGrant {
    pub project: Option<GrantedProject>,
    pub site: String,
    /// A project-local app: its folder (`<project>/.composer/apps/<name>`); `None` for the
    /// workspace's site of that name.
    pub local: Option<PathBuf>,
    /// The session it was opened from: the one it may message (ticket 0172). `None`: opened
    /// outside a chat, it messages none.
    pub session: Option<String>,
}

impl AppGrant {
    /// The app's work data (renders, view models, JSON): in the workspace, keyed by the project
    /// (decision 2026-11-07), or under `home/` for an app opened on Home.
    pub fn data(&self, workspace: &WorkspaceHome) -> PathBuf {
        let state = workspace.state();
        match &self.project {
            Some(project) => state.join("projects").join(project.name.as_str()),
            None => state.join("home"),
        }
        .join(&self.site)
    }

    /// Where it was opened, for messages: `project `game``, or `Home`.
    pub fn place(&self) -> String {
        match &self.project {
            Some(project) => format!("project `{}`", project.name),
            None => "Home".to_string(),
        }
    }
}
