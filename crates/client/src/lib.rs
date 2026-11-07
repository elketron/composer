//! What a composer client needs, and nothing of the server (plan phase 6, ticket 0180): finding
//! the server ([`Endpoint`]), a connection that reconnects and pairs requests with their replies
//! ([`Connection`]), and a store of what the server says ([`Store`]): sessions, the attached
//! session's desk and selection, terminals, the window binding. The desktop links only this.

pub mod cli;
mod connection;
mod endpoint;
mod store;
mod terminal_view;
mod window_binding;

pub use connection::{Connection, Event};
pub use endpoint::Endpoint;
pub use store::Store;
pub use terminal_view::TerminalView;
pub use window_binding::WindowBinding;
