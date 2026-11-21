//! What a parse returned: the servers that made sense, and the entries that did not.

use crate::site_mcp::SiteMcpServer;

/// What an `mcp.toml` said, read apart. A file with bad entries in it is still a file: the good
/// entries stand.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parsed {
    pub servers: Vec<SiteMcpServer>,
    /// Each entry that did not read, as a sentence naming the server and the line it starts on.
    pub errors: Vec<String>,
}

impl Parsed {
    /// Nothing was understood: there are no servers, and `errors` says why.
    pub(super) fn nothing_read(errors: Vec<String>) -> Self {
        Self {
            servers: Vec::new(),
            errors,
        }
    }
}
