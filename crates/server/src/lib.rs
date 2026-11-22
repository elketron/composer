//! The composer harness: runs agents, owns sessions, serves the WS/HTTP API.

pub mod actions;
pub mod agent;
pub mod api;
pub mod app_upstream;
pub mod attachments;

mod config;
mod config_sections;
pub mod desk;
pub mod devices;
mod error;
pub mod harness;
mod hash;
mod home;
mod hub;
mod init;
pub mod mcp;
pub mod mcp_bundles;
pub mod mcp_clients;
pub mod project;
pub mod provider;
pub mod runs;
mod secret;
mod server;
pub mod session;
pub mod site_mcp;
mod slot_guard;
mod slot_pool;
pub mod system1;
pub mod terminals;
pub mod tools;
pub mod tui;
mod viewer_upstream;
mod watcher;
pub mod watches;

pub use config::Config;
pub use config_sections::{
    AgentConfig, AppConfig, AttachmentsConfig, McpServerConfig, ProviderConfig, ServerConfig,
    System1Config,
};
pub use error::{Result, ServerError};
pub use hash::content_hash;
pub use home::Home;
pub use hub::EventHub;
pub use init::{init_home, local_token};
pub use server::Server;
pub use slot_guard::SlotGuard;
pub use slot_pool::SlotPool;
