//! `composer mcp`: a stdio MCP server offering a session's tools.
//!
//! It connects to the running server with the local token and offers the tools the session's
//! agent gets, run by the server in that session (`tool.list` / `tool.call`).
// rewrite: phase 3: becomes composer's one MCP server, not tied to a session.

mod mcp_server;
mod run_mcp;
mod server_link;
mod types;

pub use mcp_server::McpServer;
pub use run_mcp::run_mcp;
pub use server_link::ServerLink;
pub use types::{McpOptions, ToolSource};
