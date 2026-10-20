//! `composer mcp`: a stdio MCP server for external harnesses (Claude Code, …).
//!
//! It is started per ticket with the worker session fixed, connects to the running server
//! with the local token, and offers that worker's tools: the same ones a native worker of
//! its agent gets, run by the server in the worker's session (`tool.list` / `tool.call`).

mod mcp_server;
mod remote_tools;
mod run_mcp;
mod types;

pub use mcp_server::McpServer;
pub use remote_tools::RemoteTools;
pub use run_mcp::run_mcp;
pub use types::{McpOptions, ToolSource};
