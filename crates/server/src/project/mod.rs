//! A session's project folder: a git checkout outside the web, and its diff (the changes view).
// rewrite: phase 2: projects fold into sites; this keeps the folder and the changes view.

mod git_diff;
mod paths;
mod project_path;
mod semantic_diff;

pub use git_diff::{project_diff, project_diff_plain};
pub(crate) use paths::{expand_home, tilde};
pub use project_path::ProjectPath;
pub use semantic_diff::semantic_diff;
