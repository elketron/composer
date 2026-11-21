//! A site's folder (`folder:` in its `index.md`, a git checkout outside the web) and its diff:
//! the changes view.

mod git_diff;
mod paths;
mod semantic_diff;

pub use git_diff::{project_diff, project_diff_plain};
pub(crate) use paths::{expand_home, tilde};
pub use semantic_diff::semantic_diff;
