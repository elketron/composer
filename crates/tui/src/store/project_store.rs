use composer_protocol::http::{DiffComment, ProjectDiff};

use crate::diff_parse::view_lines;
use crate::store::types::DiffLine;

/// The changes view: git changes of the active project.
#[derive(Debug, Default)]
pub struct ProjectStore {
    pub open: bool,
    pub diff: Option<ProjectDiff>,
    /// What the view shows: the semantic diff where the server sent one (unless `plain`), else
    /// the text diff.
    pub lines: Vec<DiffLine>,
    /// Show the text diff even where a semantic one exists.
    pub plain: bool,
    pub error: Option<String>,
    pub loading: bool,
    /// Comments written on the diff, not sent yet.
    pub drafts: Vec<DiffComment>,
    /// The comment lines on the ticket page, as the last send left them.
    pub comments: Vec<String>,
}

impl ProjectStore {
    pub fn loaded(&mut self, diff: ProjectDiff) {
        self.loading = false;
        self.error = None;
        let ticket = |d: &ProjectDiff| d.ticket.as_ref().map(|t| t.ticket.clone());
        // Comments belong to one ticket's review; a refresh of it keeps them.
        if self.diff.as_ref().map(ticket) != Some(ticket(&diff)) {
            self.drafts.clear();
            self.comments.clear();
        }
        self.lines = view_lines(&diff, !self.plain);
        self.diff = Some(diff);
    }

    pub fn toggle_semantic(&mut self) {
        self.plain = !self.plain;
        if let Some(diff) = &self.diff {
            self.lines = view_lines(diff, !self.plain);
        }
    }

    /// Whether the server sent a semantic diff for any file.
    pub fn has_semantic(&self) -> bool {
        self.diff
            .as_ref()
            .is_some_and(|d| d.files.iter().any(|f| f.semantic.is_some()))
    }

    pub fn failed(&mut self, error: String) {
        self.loading = false;
        self.diff = None;
        self.lines.clear();
        self.error = Some(error);
    }
}
