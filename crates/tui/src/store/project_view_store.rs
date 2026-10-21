use composer_protocol::http::ProjectOverview;

/// The project view (`gp`): one project's overview, in the right pane.
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
}
