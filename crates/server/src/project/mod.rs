//! The active project: a git checkout outside the workspace.

mod git;
mod git_diff;
mod project_list;
mod project_overview;
mod ticket_branch;
mod worktree;

pub use git_diff::project_diff;
pub use project_list::{project_list, session_project};
pub use project_overview::project_overview;
pub use ticket_branch::TicketBranch;
pub use worktree::{Worktree, ticket_worktree};
