//! Sessions: transcript (client events), history (model messages), and the turn loop.

mod compaction;
mod history;
mod model_call;
#[allow(clippy::module_inception)]
mod session;
mod session_manager;
mod transcript;
mod turn;
mod turn_budget;
mod types;

pub use history::History;
pub use session::Session;
pub use session_manager::SessionManager;
pub use transcript::{Transcript, is_ephemeral};
pub use turn::{TURN_LIMIT, compact_and_wait, compact_now, submit};
pub use types::{StepOutput, TranscriptInner};
