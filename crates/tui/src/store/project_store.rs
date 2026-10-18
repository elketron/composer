use composer_protocol::http::ProjectDiff;

use crate::diff_parse::parse_diff;
use crate::store::types::DiffLine;

/// The changes view: git changes of the active project.
#[derive(Debug, Default)]
pub struct ProjectStore {
    pub open: bool,
    pub diff: Option<ProjectDiff>,
    pub lines: Vec<DiffLine>,
    pub error: Option<String>,
    pub loading: bool,
}

impl ProjectStore {
    pub fn loaded(&mut self, diff: ProjectDiff) {
        self.loading = false;
        self.error = None;
        self.lines = parse_diff(&diff.diff);
        self.diff = Some(diff);
    }

    pub fn failed(&mut self, error: String) {
        self.loading = false;
        self.diff = None;
        self.lines.clear();
        self.error = Some(error);
    }
}
