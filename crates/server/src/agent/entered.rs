use crate::site_mcp::Note;

/// What entering a site did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entered {
    /// The site's name (`3d`).
    pub site: String,
    pub title: String,
    /// What its `mcp.toml` did: approved or not, and what did not read or start.
    pub mcp: Note,
}

impl Entered {
    /// What happened besides the entering itself, in words, for a tool result or a status line.
    /// Empty when there is nothing to say.
    pub fn notes(&self) -> String {
        self.mcp.describe()
    }
}
