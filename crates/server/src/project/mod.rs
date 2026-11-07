//! The active project: a git checkout outside the workspace.

pub(crate) mod git;
mod git_diff;
mod project_list;
mod project_overview;
mod project_ownership;
mod project_path;
mod semantic_diff;
mod ticket_branch;
mod worktree;

pub use git_diff::project_diff;
pub use project_list::{project_list, session_project};
pub use project_overview::project_overview;
pub use project_ownership::ProjectOwnership;
pub use project_path::ProjectPath;
pub use semantic_diff::semantic_diff;
pub use ticket_branch::TicketBranch;
pub use worktree::{Worktree, ticket_worktree};
