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
    /// Why the last request failed, for the tab to say so. Cleared by the next attempt.
    pub error: Option<String>,
}

impl ProjectViewStore {
    pub fn loaded(&mut self, overview: ProjectOverview) {
        self.loading = false;
        self.error = None;
        self.overview = Some(overview);
    }

    /// The overview did not arrive. The tab keeps the reason instead of an endless `loading…`,
    /// and is not retried on its own: `r` asks again.
    pub fn failed(&mut self, error: String) {
        self.loading = false;
        self.error = Some(error);
    }

    /// The project to ask for now that this tab is being shown, or when `r` is pressed: only
    /// when there is nothing to read and nothing on its way. A request that failed left nothing
    /// to read, so this asks again — which is what `r` is for; nothing else calls it per key.
    pub fn reload(&mut self) -> Option<String> {
        if self.overview.is_some() || self.loading {
            return None;
        }
        self.loading = true;
        self.error = None;
        self.project.clone().filter(|project| !project.is_empty())
    }
}
