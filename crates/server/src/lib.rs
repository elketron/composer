//! The composer harness: runs agents, owns sessions, serves the WS/HTTP API.

pub mod actions;
pub mod agent;
pub mod api;
pub mod apps;
pub mod attachments;

mod config;
mod config_sections;
pub mod desk;
pub mod devices;
mod error;
pub mod harness;
mod hash;
mod home;
mod host;
mod host_routes;
mod host_state;
mod hub;
mod init;
mod launcher;
pub mod library;
pub mod mcp;
pub mod mcp_clients;
mod migrate;
pub mod pipeline;
pub mod project;
pub mod provider;
pub mod reminders;
pub mod sandbox;
mod server;
pub mod session;
mod slot_guard;
mod slot_pool;
mod stock_root;
mod template_name;
pub mod terminals;
pub mod tools;
mod update;
mod watcher;
pub mod watches;
pub mod worker;
mod workspace_create;
mod workspace_home;
mod workspace_name;
mod workspace_template;
mod workspace_template_file;

pub use config::Config;
pub use config_sections::{
    AgentConfig, AppsConfig, AttachmentsConfig, ClaudeCodeConfig, LauncherEntry, McpServerConfig,
    PipelinesConfig, ProjectSettings, ProjectsConfig, ProviderConfig, RemindersConfig,
    SandboxConfig, ServerConfig,
};
pub use error::{Result, ServerError};
pub use hash::content_hash;
pub use home::Home;
pub use host::Host;
pub use host_state::Starter;
pub use hub::EventHub;
pub use init::{init_home, init_workspace, local_token};
pub use launcher::LauncherInfo;
pub use migrate::migrate_home;
pub use server::Server;
pub use slot_guard::SlotGuard;
pub use slot_pool::SlotPool;
pub use template_name::TemplateName;
pub use update::{UpdateItem, UpdateOutcome, pending_updates, update_home};
pub use workspace_create::create_workspace;
pub use workspace_home::WorkspaceHome;
pub use workspace_name::WorkspaceName;
pub use workspace_template::WorkspaceTemplate;
