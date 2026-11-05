//! Watches: a session asks to hear when files in a site change (the `watch` tool); a matching
//! change, whoever made it (an editor, an app, another agent, a process outside composer), is
//! posted to the session as a message, like a reminder going off. Changes a session makes
//! itself through its own tools are not reported back to it.

mod delivery;
mod types;
mod watch_registry;

pub use delivery::run_watches;
pub use types::{FileWatch, WatchHit};
pub use watch_registry::WatchRegistry;

tokio::task_local! {
    /// The session whose tool call is running: its own writes don't trigger its watches.
    pub static WRITER: String;
}
