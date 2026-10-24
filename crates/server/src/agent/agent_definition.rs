use composer_core::{Index, SiteUrl, Workspace};
use serde_json::Value;

use crate::agent::SiteAccess;
use crate::error::{Result, ServerError};

/// An agent page: frontmatter configures it, the body is its system prompt.
#[derive(Debug, Clone)]
pub struct AgentDefinition {
    pub name: String,
    pub url: SiteUrl,
    /// Tool names from `tools:`; `None` means every available tool.
    pub tools: Option<Vec<String>>,
    pub sites: SiteAccess,
    /// `role:`; `worker` marks an agent `spawn_worker` can start.
    pub role: Option<String>,
    pub title: String,
    /// `description:`, one line for lists (else the title).
    pub description: String,
    pub prompt: String,
}

impl AgentDefinition {
    pub fn load(workspace: &Workspace, name: &str) -> Result<Self> {
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if !valid {
            return Err(ServerError::AgentNotFound(name.to_string()));
        }
        let url = SiteUrl::new("agents", name)?;
        let page = workspace
            .read_page(&url)
            .map_err(|_| ServerError::AgentNotFound(name.to_string()))?;
        let tools = page
            .frontmatter
            .get("tools")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            });
        Ok(Self {
            name: name.to_string(),
            url: page.url.clone(),
            tools,
            sites: SiteAccess::from_frontmatter(page.frontmatter.get("sites")),
            role: page
                .frontmatter
                .get_str("role")
                .map(|r| r.trim().to_lowercase())
                .filter(|r| !r.is_empty()),
            title: page.title(),
            description: page
                .frontmatter
                .get_str("description")
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| page.title().to_string()),
            prompt: page.body().trim().to_string(),
        })
    }

    /// Can run a ticket: `role: worker`, or (pages from before `role:`) no role and the
    /// `finish` tool.
    pub fn is_worker(&self) -> bool {
        match self.role.as_deref() {
            Some(role) => role == "worker" || role == "stage",
            None => self
                .tools
                .as_ref()
                .is_some_and(|tools| tools.iter().any(|t| t == "finish")),
        }
    }

    /// A worker that runs pipeline stages only (`role: stage`).
    pub fn is_stage(&self) -> bool {
        self.role.as_deref() == Some("stage")
    }

    /// Every worker agent in `site://agents` `spawn_worker` can start, by name (not the
    /// stage ones).
    pub fn workers(workspace: &Workspace, index: &Index) -> Vec<Self> {
        Self::all(workspace, index)
            .into_iter()
            .filter(|a| a.is_worker() && !a.is_stage())
            .collect()
    }

    /// Every agent in `site://agents` (`type: agent` pages), by name.
    pub fn all(workspace: &Workspace, index: &Index) -> Vec<Self> {
        let mut names: Vec<String> = index
            .entries()
            .filter(|e| e.url.site() == "agents" && e.frontmatter.matches("type", "agent"))
            .filter_map(|e| {
                let file = e.url.path().rsplit('/').next()?;
                Some(file.strip_suffix(".md").unwrap_or(file).to_string())
            })
            .collect();
        names.sort();
        names
            .iter()
            .filter_map(|name| Self::load(workspace, name).ok())
            .collect()
    }

    pub fn allows_tool(&self, tool: &str) -> bool {
        self.tools
            .as_ref()
            .is_none_or(|tools| tools.iter().any(|t| t == tool))
    }

    /// Whether `tools:` lists this MCP tool (`github__get_issue`), by name or as `github__*`.
    /// Never without a `tools:` list: MCP tools are opt-in.
    pub fn allows_mcp_tool(&self, tool: &str) -> bool {
        self.tools.as_ref().is_some_and(|tools| {
            tools
                .iter()
                .any(|t| t.contains("__") && crate::mcp_clients::glob_matches(t, tool))
        })
    }

    /// The MCP servers whose tools `tools:` lists (`github` for `github__*`).
    pub fn mcp_servers(&self) -> Vec<String> {
        let mut servers: Vec<String> = self
            .tools
            .iter()
            .flatten()
            .filter_map(|t| t.split_once("__").map(|(server, _)| server.to_string()))
            .collect();
        servers.sort();
        servers.dedup();
        servers
    }
}
