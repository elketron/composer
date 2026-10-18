//! Sessions: transcript (client events), history (model messages), and the turn loop.

mod history;
#[allow(clippy::module_inception)]
mod session;
mod session_manager;
mod transcript;
mod turn;
mod types;

pub use history::History;
pub use session::Session;
pub use session_manager::SessionManager;
pub use transcript::{Transcript, is_ephemeral};
pub use turn::submit;
pub use types::TranscriptInner;
