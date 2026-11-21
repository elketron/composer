//! What runs outside composer's own agent loop: child processes in their own process group (and
//! systemd scope), shell-command approval policy, and the user's "always" rules.
// rewrite: phase 3: the base for the ACP harness and background processes.

mod child_process;
mod command_policy;
mod tool_rules;
mod worker_scope;

pub use child_process::ChildProcess;
#[cfg(unix)]
pub use child_process::signal_group;
pub use command_policy::{CommandContext, Verdict, check_command, rule_for_words};
pub use tool_rules::{ToolRules, rule_for};
pub use worker_scope::WorkerScope;
