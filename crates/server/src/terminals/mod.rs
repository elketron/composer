//! Server-owned terminals (plan phase 5, ticket 0176): a session's shells, streamed to its
//! clients; the agent reads them, and types into one only when the user approves. The agent
//! also starts background processes in terminals of their own (`process_start`).

mod background;
mod scrollback;
mod terminal;
mod terminal_registry;
mod types;

pub use background::EXIT_LINES;
pub use scrollback::Scrollback;
pub use terminal::Terminal;
pub use terminal_registry::{Terminals, owned};
pub use types::{Background, StoppedBy};
