//! An approved `mcp.toml` made into servers: what sessions in the site use.

use crate::McpServerConfig;

/// One server of a site, as `[mcp.*]` would have described it (its `allow` and `read_only`
/// included).
#[derive(Debug, Clone, PartialEq)]
pub struct SiteServer {
    pub name: String,
    pub config: McpServerConfig,
}

/// The servers of one site's approved `mcp.toml`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SiteServers {
    pub site: String,
    /// The hash of the bytes the user approved.
    pub hash: String,
    pub servers: Vec<SiteServer>,
    /// The servers that could not be made (a bundle that is not installed, a name `config.toml`
    /// already uses), each a sentence.
    pub errors: Vec<String>,
}
