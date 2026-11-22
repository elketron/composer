//! Composer's runs as MCP clients: the servers in `config.toml` (`[mcp.<name>]`, in every session)
//! and those of the run's site (its approved `mcp.toml`, [`crate::site_mcp`]), their tools named
//! for their server (`github__*`), each call approved unless the server's `allow` or an `always`
//! covers it. (`crate::mcp` is the other direction: composer's tools for any harness.)

mod http_transport;
mod mcp_client;
mod mcp_event;
mod mcp_events;
mod mcp_servers;
mod stdio_transport;
mod types;

pub use mcp_client::{McpClient, expand, output_text};
pub use mcp_event::{EVENT_METHOD, McpEvent, SESSION_META};
pub use mcp_events::{COALESCE, McpEvents};
pub use mcp_servers::McpServers;
pub use types::{EventSink, RemoteTool, Transport, glob_matches, tool_name};
