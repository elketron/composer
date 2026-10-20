//! The active project: a git checkout outside the workspace.

mod git;
mod git_diff;
mod ticket_branch;
mod worktree;

pub use git_diff::project_diff;
pub use ticket_branch::TicketBranch;
pub use worktree::{Worktree, ticket_worktree};
