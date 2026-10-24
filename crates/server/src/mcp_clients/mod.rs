//! Composer's agents as MCP clients: the servers in `config.toml` (`[mcp.<name>]`), their tools
//! for the agents that list them (`github__*`), each call approved unless the server's `allow`
//! or an `always` covers it. (`crate::mcp` is the other direction: composer's tools for a
//! worker's Claude Code run.)

mod http_transport;
mod mcp_client;
mod mcp_servers;
mod stdio_transport;
mod types;

pub use mcp_client::{McpClient, expand, output_text};
pub use mcp_servers::McpServers;
pub use types::{RemoteTool, Transport, glob_matches, tool_name};
