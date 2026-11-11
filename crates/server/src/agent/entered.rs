use crate::agent::AppLaunch;

/// What entering a site did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entered {
    /// The site's name (`3d`).
    pub site: String,
    pub title: String,
    /// What happened to the site's `app:`.
    pub app: AppLaunch,
    /// The live view tab was put on the session's desk (the application runs and the workspace
    /// has a viewer).
    pub live_view: bool,
    /// The MCP servers the site names in `mcp:` whose tools it does not get, each as a sentence
    /// saying why (`[mcp.<server>] sites` does not list the site, or there is no such server).
    pub mcp_refused: Vec<String>,
}

impl Entered {
    /// What happened besides the entering itself, in words, for a tool result or a status line:
    /// the application's outcome and the MCP servers the site did not get. Empty when neither.
    pub fn notes(&self) -> String {
        let mut notes = vec![self.app.describe()];
        notes.extend(
            self.mcp_refused
                .iter()
                .map(|refused| format!("Note: {refused}.")),
        );
        notes.retain(|note| !note.is_empty());
        notes.join(" ")
    }
}
