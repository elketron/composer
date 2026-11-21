//! Sessions: transcript (client events), history (model messages), and runs: `run(kind)` and
//! the sinks it writes to.

mod compaction;
mod history;
mod memory_sink;
mod model_call;
mod run;
mod run_sink;
#[allow(clippy::module_inception)]
mod session;
mod session_folder;
mod session_manager;
mod session_sink;
mod transcript;
mod turn;
mod turn_budget;
mod types;

pub use history::History;
pub use memory_sink::MemorySink;
pub use run::{run, run_once};
pub use run_sink::RunSink;
pub use session::Session;
pub use session_folder::SessionFolder;
pub use session_manager::SessionManager;
pub use session_sink::SessionSink;
pub use transcript::{FULL_PARSES, Transcript, is_ephemeral};
pub use turn::{compact_now, submit};
pub use types::{StepOutput, TranscriptInner};
