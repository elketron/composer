//! `composer mcp --session <id>`: composer's MCP server for any harness, on stdio.
//!
//! It connects to the running server with the local token and offers the tools a run of the
//! session's kind has in its site — the entry tools, the site's tools, `config.toml`'s MCP servers
//! and the site's approved `mcp.toml` servers — run by the server in that session (`tool.list` /
//! `tool.call`). The list changes with the site: when the session's site is set (or its servers
//! come up), the server says `tools.changed` and the harness hears
//! `notifications/tools/list_changed`.

mod mcp_server;
mod run_mcp;
mod server_link;
mod types;

pub use mcp_server::McpServer;
pub use run_mcp::run_mcp;
pub use server_link::ServerLink;
pub use types::{McpOptions, ToolSource};
