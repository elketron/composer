//! Harnesses: what works a ticket.
//!
//! `spawn_worker` creates the worker session and claims the ticket, then hands the work to a
//! harness: composer's own agent loop (`composer worker`), and later external CLIs. A harness
//! starts a run; the run's events and its `finish` reach the worker session through a
//! [`WorkerSink`], and the registry can interrupt it or pass it a message.

mod child_process;
mod own_harness;
mod resequence;
mod types;
mod worker_sink;

pub use child_process::ChildProcess;
pub use own_harness::OwnHarness;
pub use types::{Harness, HarnessRun, StartRequest};
pub use worker_sink::WorkerSink;
