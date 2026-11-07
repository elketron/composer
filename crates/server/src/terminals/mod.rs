//! Server-owned terminals (plan phase 5, ticket 0176): a session's shells, in its workspace's
//! sandbox, streamed to its clients; the agent reads them, and types into one only when the user
//! approves.

mod scrollback;
mod terminal;
mod terminal_registry;

pub use scrollback::Scrollback;
pub use terminal::Terminal;
pub use terminal_registry::{Terminals, owned};
