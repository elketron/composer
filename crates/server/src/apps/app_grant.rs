use std::path::PathBuf;

use crate::apps::ProjectName;

/// What an app token lets an app do: it is `site`'s app, opened on `project` (`folder`). It reads
/// the workspace's web; it writes only its own site and the project, and runs only its own site's
/// actions (decision 2026-11-07).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppGrant {
    pub project: ProjectName,
    pub folder: PathBuf,
    pub site: String,
}
