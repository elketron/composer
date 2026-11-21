//! One server a site declares.

use crate::mcp_clients::glob_matches;
use crate::site_mcp::McpTarget;
use crate::site_mcp::describe::word;

/// One `[servers.<name>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteMcpServer {
    /// As it is spelled in the file: its tools are `<name>__<tool>`.
    pub name: String,
    pub target: McpTarget,
    /// Its tools that run without asking (names or `prefix*`).
    pub allow: Vec<String>,
    /// Its tools workflow runs get too; none by default.
    pub read_only: Vec<String>,
}

impl SiteMcpServer {
    /// Whether `tool` (a name as its server lists it) runs without asking.
    pub fn allows_tool(&self, tool: &str) -> bool {
        self.allow.iter().any(|pattern| glob_matches(pattern, tool))
    }

    /// What this server would do, in one line, for the user to read before approving the file.
    pub fn describes(&self) -> String {
        let mut said = format!("{}: {}", word(&self.name), self.target.describes());
        let list = |patterns: &[String]| {
            patterns
                .iter()
                .map(|p| word(p))
                .collect::<Vec<_>>()
                .join(", ")
        };
        if !self.allow.is_empty() {
            said.push_str(&format!("; tools {} run without asking", list(&self.allow)));
        }
        if !self.read_only.is_empty() {
            said.push_str(&format!(
                "; tools {} are offered to workflow runs",
                list(&self.read_only)
            ));
        }
        said
    }
}
