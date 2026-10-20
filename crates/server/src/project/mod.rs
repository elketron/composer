//! The active project: a git checkout outside the workspace.

mod git_diff;
mod worktree;

pub use git_diff::project_diff;
pub use worktree::{Worktree, ticket_worktree};
