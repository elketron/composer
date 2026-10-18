//! The composer harness: runs agents, owns sessions, serves the WS/HTTP API.

pub mod actions;
pub mod agent;
pub mod api;
pub mod client;
mod config;
mod config_sections;
pub mod devices;
mod error;
mod hash;
mod home;
mod hub;
mod init;
pub mod project;
pub mod provider;
mod server;
pub mod session;
mod slot_guard;
mod slot_pool;
pub mod tools;
mod watcher;
pub mod worker;

pub use config::Config;
pub use config_sections::{AgentConfig, ProviderConfig, ServerConfig};
pub use error::{Result, ServerError};
pub use hash::content_hash;
pub use home::Home;
pub use hub::EventHub;
pub use init::{init_home, local_token};
pub use server::Server;
pub use slot_guard::SlotGuard;
pub use slot_pool::SlotPool;
