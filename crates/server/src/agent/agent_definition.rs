use composer_core::{SiteUrl, Workspace};
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
            prompt: page.body().trim().to_string(),
        })
    }

    pub fn allows_tool(&self, tool: &str) -> bool {
        self.tools
            .as_ref()
            .is_none_or(|tools| tools.iter().any(|t| t == tool))
    }
}
