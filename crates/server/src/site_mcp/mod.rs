//! A site's `mcp.toml`: the MCP servers a session gets while it is in the site, and the approval
//! that keeps an unapproved file from starting anything.
//!
//! ```toml
//! [servers.blender]
//! bundle = "blender"              # an installed .mcpb (<home>/mcps/blender.mcpb)
//! allow = ["get_*"]               # tools that run without asking
//! read_only = ["get_*"]           # tools workflow runs get too (none by default)
//!
//! [servers.github]
//! command = "github-mcp-server"   # a stdio server
//! args = ["stdio"]
//! env = { GITHUB_PERSONAL_ACCESS_TOKEN = "${GITHUB_TOKEN}" }
//!
//! [servers.docs]
//! url = "https://example.com/mcp" # a streamable-HTTP server
//! headers = { Authorization = "Bearer ${DOCS_TOKEN}" }
//! ```
//!
//! It is a file an agent can write, which is why nothing in it starts until the user approved
//! exactly these bytes ([`ensure_approved`]): the approval is kept in the recipe approval store,
//! keyed by the file's hash, so it survives restarts and an edit asks again. It is only ever asked
//! of the user's clients.
//!
//! The files, in the order the work goes:
//!
//! - `site_file.rs`      the bytes on disk and their hash
//! - `mcp_file.rs`       the pure read: bytes in, servers and errors out
//! - `site_mcp_server.rs` one declared server; `mcp_target.rs` how it is reached
//! - `describe.rs`       the words the user reads before approving
//! - `approve.rs`        the gate: ask, remember; `approved_mcp.rs` what it hands back
//! - `resolve.rs`        an approved file as `[mcp.*]`-shaped configs ([`SiteServers`])
//! - `active_sites.rs`   the sites whose servers are in use, by the hash approved
//! - `enter.rs`          what setting a site does with all of it; `note.rs` what it says

mod active_sites;
mod approve;
mod approved_mcp;
mod describe;
mod enter;
mod mcp_file;
mod mcp_target;
mod note;
mod parsed;
mod resolve;
mod site_file;
mod site_mcp_server;
mod site_servers;

pub use active_sites::ActiveSites;
pub use approve::{Gate, approved_now, ensure_approved};
pub use approved_mcp::ApprovedMcp;
pub use enter::{active, enter};
pub use mcp_file::{MAX_BYTES, parse, valid_name};
pub use mcp_target::McpTarget;
pub use note::Note;
pub use parsed::Parsed;
pub use resolve::resolve;
pub use site_file::{MCP_FILE, SiteMcpFile, load};
pub use site_mcp_server::SiteMcpServer;
pub use site_servers::{SiteServer, SiteServers};
