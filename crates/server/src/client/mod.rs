//! Minimal command-line clients of the server, until the TUI exists.

mod apps;
mod ask;
mod harness;
mod library;
mod ticket;

pub use apps::{local_app, rebuild_apps};
pub use ask::{AskOptions, ask};
pub use harness::harness;
pub use library::{LibraryAction, library};
pub use ticket::{TicketAction, ticket};
