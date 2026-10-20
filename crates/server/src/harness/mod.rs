//! Harnesses: what works a ticket.
//!
//! `spawn_worker` creates the worker session and claims the ticket, then hands the work to a
//! harness: composer's own agent loop (`composer worker`) or the Claude Code CLI. A harness
//! starts a run; the run's events and its `finish` reach the worker session through a
//! [`WorkerSink`], and the registry can interrupt it or pass it a message.

mod child_process;
mod claude_code_harness;
mod claude_code_run;
mod own_harness;
mod project_policy;
mod resequence;
mod stream_json;
mod types;
mod worker_sink;

pub use child_process::ChildProcess;
pub use claude_code_harness::ClaudeCodeHarness;
pub use claude_code_run::ClaudeCodeRun;
pub use own_harness::OwnHarness;
pub use project_policy::{check_harness_allowed, project_settings};
pub use types::{Harness, HarnessRun, Pause, Resume, StartRequest};
pub use worker_sink::WorkerSink;
