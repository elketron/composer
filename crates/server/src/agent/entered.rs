/// What entering a site did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entered {
    /// The site's name (`3d`).
    pub site: String,
    pub title: String,
    /// The MCP servers the site names in `mcp:` whose tools it does not get, each as a sentence
    /// saying why (`[mcp.<server>] sites` does not list the site, or there is no such server).
    pub mcp_refused: Vec<String>,
}

impl Entered {
    /// What happened besides the entering itself, in words, for a tool result or a status line:
    /// the MCP servers the site did not get. Empty when none.
    pub fn notes(&self) -> String {
        self.mcp_refused
            .iter()
            .map(|refused| format!("Note: {refused}."))
            .collect::<Vec<_>>()
            .join(" ")
    }
}
