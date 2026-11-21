//! The proof the gate handed back: these servers, from these bytes, which the user approved.

use crate::site_mcp::SiteMcpServer;

/// An approved `mcp.toml`, as far as anything that starts its servers is concerned.
///
/// Only the gate ([`crate::site_mcp::ensure_approved`], [`crate::site_mcp::approved_now`]) makes
/// one, and only for bytes the user said yes to: what [`crate::site_mcp::resolve`] turns into
/// servers is this, never a fresh read of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedMcp {
    site: String,
    hash: String,
    servers: Vec<SiteMcpServer>,
}

impl ApprovedMcp {
    /// Only the gate calls this: `pub(super)`, so nothing else in the crate can build an approval
    /// out of a file nobody answered.
    pub(super) fn new(site: &str, hash: &str, servers: Vec<SiteMcpServer>) -> Self {
        Self {
            site: site.to_string(),
            hash: hash.to_string(),
            servers,
        }
    }

    /// The site this was read from.
    pub fn site(&self) -> &str {
        &self.site
    }

    /// sha256 of the file these servers came from: what the approval covers.
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// The servers, in the order the file lists them.
    pub fn servers(&self) -> &[SiteMcpServer] {
        &self.servers
    }
}
