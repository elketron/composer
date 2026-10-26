//! Harnesses: what works a ticket.
//!
//! `spawn_worker` creates the worker session and claims the ticket, then hands the work to a
//! harness: composer's own agent loop (`composer worker`) or the Claude Code CLI. A harness
//! starts a run; the run's events and its `finish` reach the worker session through a
//! [`WorkerSink`], and the registry can interrupt it or pass it a message.

mod child_process;
mod claude_code_harness;
mod command_policy;
mod claude_code_run;
mod harness_switches;
mod own_harness;
mod project_policy;
mod resequence;
mod stream_json;
mod tool_permission;
mod tool_rules;
mod types;
mod worker_sink;

pub use child_process::ChildProcess;
#[cfg(unix)]
pub use child_process::signal_group;
pub use claude_code_harness::ClaudeCodeHarness;
pub use claude_code_run::ClaudeCodeRun;
pub use harness_switches::HarnessSwitches;
pub use own_harness::OwnHarness;
pub use project_policy::{check_harness_allowed, pick_model, project_settings};
pub use tool_permission::{PERMISSION_TOOL, answer_permission, permission_tool_info};
pub use tool_rules::ToolRules;
pub use types::{Harness, HarnessRun, Pause, Resume, StartRequest};
pub use worker_sink::WorkerSink;
