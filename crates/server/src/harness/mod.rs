//! What runs outside composer's own agent loop: outside agents over the Agent Client Protocol
//! (`[acp.<name>]`, a session's `harness`), child processes in their own process group (and
//! systemd scope), and the user's "always" rules.

mod acp_agent;
mod acp_agents;
mod acp_config;
mod acp_turn;
mod child_process;
mod tool_rules;
mod worker_scope;

pub use acp_agent::AcpAgent;
pub use acp_agents::{AcpAgents, run_acp_turn};
pub use acp_config::AcpConfig;
pub use acp_turn::AcpTurn;
pub use child_process::ChildProcess;
#[cfg(unix)]
pub use child_process::signal_group;
pub use tool_rules::ToolRules;
pub use worker_scope::WorkerScope;
