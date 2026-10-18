//! Orchestration: tickets and worker processes.
//!
//! The orchestrator's `spawn_worker` starts `composer worker` as a child process. The
//! worker runs its own agent loop on one ticket and talks to the parent over WebSocket:
//! model slots and approvals come from the parent, its session events go to the parent
//! (and from there to every client), and `finish` ends it.

mod parent_link;
mod run_worker;
pub mod ticket;
mod types;
mod worker_registry;

pub use parent_link::ParentLink;
pub use run_worker::{WorkerOptions, run_worker};
pub use types::{Outbound, WorkerHandle};
pub use worker_registry::WorkerRegistry;
