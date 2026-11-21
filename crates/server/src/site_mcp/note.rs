//! What setting a site says about its `mcp.toml`.

use crate::site_mcp::MCP_FILE;

/// What setting a site found in its `mcp.toml`, for the note that comes with it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Note {
    /// The user approved the bytes that are there now.
    pub approved: bool,
    /// The servers the file declares, by name.
    pub servers: Vec<String>,
    /// What did not read or could not be started, each a sentence.
    pub errors: Vec<String>,
}

impl Note {
    /// Words about it: whether its servers are in use, and what went wrong. Empty when there is
    /// nothing to say (no file, or one that declares nothing).
    pub fn describe(&self) -> String {
        let mut notes = Vec::new();
        if !self.servers.is_empty() {
            let names = self.servers.join(", ");
            notes.push(if self.approved {
                format!("Its {MCP_FILE} is approved: MCP servers {names}.")
            } else {
                format!(
                    "Note: its {MCP_FILE} (MCP servers {names}) is not approved, so none of them \
                     started."
                )
            });
        }
        notes.extend(self.errors.iter().map(|error| format!("Note: {error}.")));
        notes.join(" ")
    }
}
