//! Site actions: `justfile` recipes, approved once per version.

mod approval_wait;
mod approvals;
mod check_args;
mod ensure_approved;
mod justfile;
mod pending_approvals;
mod run;
mod types;

pub use approval_wait::ApprovalWait;
pub use approvals::Approvals;
pub use check_args::check_args;
pub use ensure_approved::ensure_approved;
pub use justfile::Justfile;
pub use pending_approvals::PendingApprovals;
pub use run::{RecipeEnv, run_recipe};
pub use types::{Approval, ApprovalAnswer, Recipe, Refusal, RunOutput};
