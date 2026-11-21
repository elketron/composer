use composer_core::{Index, SiteUrl, Web};
use serde_json::Value;

use crate::agent::SiteAccess;
use crate::error::{Result, ServerError};

/// The built-in agent's system prompt.
const BUILTIN_PROMPT: &str = "You are composer's agent. You work with the user in their web of \
pages (sites, `site://`) and in their projects. Open and search pages before you answer from \
memory; write what lasts (decisions, facts about the user) into their pages. Keep answers short.";

/// An agent page: frontmatter configures it, the body is its system prompt.
#[derive(Debug, Clone)]
pub struct AgentDefinition {
    pub name: String,
    pub url: SiteUrl,
    /// Tool names from `tools:`; `None` means every available tool.
    pub tools: Option<Vec<String>>,
    pub sites: SiteAccess,
    pub title: String,
    /// `description:`, one line for lists (else the title).
    pub description: String,
    pub prompt: String,
    /// `reasoning:` (none, low, medium, high): how much the model thinks, sent as
    /// `reasoning_effort`; else `[agent] reasoning_effort`, else the provider's own.
    pub reasoning: Option<String>,
}

impl AgentDefinition {
    pub fn load(workspace: &Web, name: &str) -> Result<Self> {
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if !valid {
            return Err(ServerError::AgentNotFound(name.to_string()));
        }
        let url = SiteUrl::new("agents", name)?;
        let page = match workspace.read_page(&url) {
            Ok(page) => page,
            Err(_) if name == crate::agent::DEFAULT_AGENT => return Ok(Self::builtin()),
            Err(_) => return Err(ServerError::AgentNotFound(name.to_string())),
        };
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
            title: page.title(),
            description: page
                .frontmatter
                .get_str("description")
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| page.title().to_string()),
            prompt: page.body().trim().to_string(),
            reasoning: page
                .frontmatter
                .get_str("reasoning")
                .map(|r| r.trim().to_lowercase())
                .filter(|r| !r.is_empty()),
        })
    }

    /// The built-in [`DEFAULT_AGENT`](crate::agent::DEFAULT_AGENT): every tool, every site.
    // rewrite: phase 2: agent definitions become run settings.
    pub fn builtin() -> Self {
        Self {
            name: crate::agent::DEFAULT_AGENT.to_string(),
            url: SiteUrl::new("agents", &format!("{}.md", crate::agent::DEFAULT_AGENT))
                .expect("a valid url"),
            tools: None,
            sites: SiteAccess::All,
            title: "Composer".into(),
            description: "works with you in your web of pages and your projects".into(),
            prompt: BUILTIN_PROMPT.into(),
            reasoning: None,
        }
    }

    /// The built-in agent, then every agent in `site://agents` (`type: agent` pages), by name.
    pub fn all(workspace: &Web, index: &Index) -> Vec<Self> {
        let mut names: Vec<String> = index
            .entries()
            .filter(|e| e.url.site() == "agents" && e.frontmatter.matches("type", "agent"))
            .filter_map(|e| {
                let file = e.url.path().rsplit('/').next()?;
                Some(file.strip_suffix(".md").unwrap_or(file).to_string())
            })
            .collect();
        names.sort();
        let mut agents: Vec<Self> = names
            .iter()
            .filter_map(|name| Self::load(workspace, name).ok())
            .collect();
        if !agents.iter().any(|a| a.name == crate::agent::DEFAULT_AGENT) {
            agents.insert(0, Self::builtin());
        }
        agents
    }

    pub fn allows_tool(&self, tool: &str) -> bool {
        self.tools
            .as_ref()
            .is_none_or(|tools| tools.iter().any(|t| t == tool))
    }

    /// Whether `tools:` lists this MCP tool (`github__get_issue`), by name or as `github__*`.
    /// Never without a `tools:` list: MCP tools are opt-in. A site may add more: whether a tool
    /// is available is [`crate::mcp_clients::McpServers::allows`]'s to say.
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
