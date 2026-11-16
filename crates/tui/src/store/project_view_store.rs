use composer_protocol::http::ProjectOverview;

/// The project view (`gp`): one project's overview, as a tab of its own (the desk has no kind
/// for it, so this tab is never written to it).
#[derive(Debug, Default)]
pub struct ProjectViewStore {
    pub open: bool,
    /// What was asked for (a folder or a name).
    pub project: Option<String>,
    pub overview: Option<ProjectOverview>,
    pub loading: bool,
}

impl ProjectViewStore {
    pub fn loaded(&mut self, overview: ProjectOverview) {
        self.loading = false;
        self.overview = Some(overview);
    }

    /// The project to ask for now that this tab is being shown: only when there is nothing to
    /// read and nothing on its way. The same answer serves `r`: an overview that arrived is not
    /// stale, and a request that failed says so in the view instead of being retried per key.
    pub fn reload(&mut self) -> Option<String> {
        if self.overview.is_some() || self.loading {
            return None;
        }
        self.loading = true;
        self.project.clone().filter(|project| !project.is_empty())
    }
}
