//! Site actions: `justfile` recipes, approved once per version.

mod approvals;
mod justfile;
mod pending_approvals;
mod run;
mod types;

pub use approvals::Approvals;
pub use justfile::Justfile;
pub use pending_approvals::PendingApprovals;
pub use run::{RecipeEnv, run_recipe};
pub use types::{Approval, Recipe, RunOutput};
