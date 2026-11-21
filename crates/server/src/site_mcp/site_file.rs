//! A site's `mcp.toml` as it sits on disk: its bytes, their hash, and what they declare.
//!
//! Read afresh every time, because an agent can write the file: an approval is only good for the
//! bytes it was shown, and it is recorded against their hash, never against the file's name.

use std::path::PathBuf;

use composer_core::{SiteUrl, Web};

use crate::content_hash;
use crate::site_mcp::{SiteMcpServer, parse};

/// The file's name, as it is named in an approval and in every message about it.
pub const MCP_FILE: &str = "mcp.toml";

/// A site's `mcp.toml` as it is on disk right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteMcpFile {
    /// The file it was read from, named in the approval.
    pub path: PathBuf,
    /// Its text, so the user reads the file itself and not only what was made of it.
    pub text: String,
    /// sha256 of its bytes: what the approval is recorded against.
    pub hash: String,
    pub servers: Vec<SiteMcpServer>,
    /// What did not read, each naming its server and line.
    pub errors: Vec<String>,
}

/// The site's `mcp.toml`, or `None` when it has none.
pub fn load(web: &Web, site: &str) -> Option<SiteMcpFile> {
    let path = web.locate(&SiteUrl::new(site, MCP_FILE).ok()?).ok()?;
    let bytes = std::fs::read(&path).ok()?;
    let parsed = parse(&bytes, &path);
    Some(SiteMcpFile {
        path,
        text: String::from_utf8_lossy(&bytes).into_owned(),
        hash: content_hash(&bytes),
        servers: parsed.servers,
        errors: parsed.errors,
    })
}
