//! A site as a capability: its `index.md` is the prompt and its `tools:` are the verbs that
//! exist only while the agent is inside it.

use std::path::PathBuf;

use composer_core::{SiteUrl, Web};
use serde_json::Value;

use crate::project::expand_home;

/// A site the agent can enter (`enter_site`).
#[derive(Debug, Clone)]
pub struct Site {
    pub name: String,
    pub title: String,
    /// The tools this site adds to the base set.
    pub tools: Vec<String>,
    /// `mcp:`: the MCP servers the page names. What the page says, not what is granted: a
    /// server's tools come with the site only where `[mcp.<server>] sites` in `config.toml` lists
    /// this site ([`crate::mcp_clients::McpServers::allows`]).
    pub mcp: Vec<String>,
    /// `folder:`: a folder outside the web the site works on (`~/Projects/composer`), `~`
    /// expanded. Terminals start there and the changes view diffs it.
    pub folder: Option<PathBuf>,
    /// The site's `index.md` body, put in the prompt after the agent's.
    pub prompt: String,
}

impl Site {
    pub fn load(web: &Web, name: &str) -> Result<Self, String> {
        let name = name
            .trim()
            .trim_start_matches("site://")
            .trim_end_matches('/');
        let url = SiteUrl::new(name, "").map_err(|e| e.to_string())?;
        let page = web
            .read_page(&url)
            .map_err(|_| format!("no site://{name}"))?;
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
            })
            .unwrap_or_default();
        let mcp = match page.frontmatter.get("mcp") {
            Some(Value::String(server)) => vec![server.clone()],
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        }
        .into_iter()
        .map(|server| server.trim().to_string())
        .filter(|server| !server.is_empty())
        .collect();
        let folder = page
            .frontmatter
            .get_str("folder")
            .map(|folder| folder.trim().to_string())
            .filter(|folder| !folder.is_empty())
            .map(|folder| expand_home(&folder));
        Ok(Self {
            name: name.to_string(),
            title: page.title(),
            tools,
            mcp,
            folder,
            prompt: page.body().trim().to_string(),
        })
    }

    pub fn allows_tool(&self, tool: &str) -> bool {
        self.tools.iter().any(|t| t == tool)
    }

    /// Its folder, when it names one that exists: canonical.
    pub fn existing_folder(&self) -> Result<PathBuf, String> {
        let folder = self
            .folder
            .as_ref()
            .ok_or_else(|| format!("site://{} names no folder", self.name))?;
        let canonical = folder
            .canonicalize()
            .map_err(|e| format!("{}: {e}", crate::project::tilde(folder)))?;
        match canonical.is_dir() {
            true => Ok(canonical),
            false => Err(format!("{} is not a folder", crate::project::tilde(folder))),
        }
    }

    pub fn url(&self) -> SiteUrl {
        SiteUrl::new(&self.name, "").expect("a loaded site name is valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use composer_core::Web;

    fn site(frontmatter: &str) -> Site {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("lab")).unwrap();
        std::fs::write(
            dir.path().join("lab/index.md"),
            format!("---\ntitle: Lab\n{frontmatter}---\nBody.\n"),
        )
        .unwrap();
        Site::load(&Web::new(dir.path()), "lab").unwrap()
    }

    #[test]
    fn mcp_is_a_list_or_a_single_string() {
        assert_eq!(
            site("mcp: [blender, comfyui]\n").mcp,
            ["blender", "comfyui"]
        );
        assert_eq!(site("mcp: blender\n").mcp, ["blender"]);
        assert!(site("").mcp.is_empty());
    }

    #[test]
    fn folder_expands_home() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            site("folder: ~/Projects/x\n").folder,
            Some(std::path::Path::new(&home).join("Projects/x"))
        );
        assert_eq!(site("folder: /srv/x\n").folder, Some("/srv/x".into()));
        assert_eq!(site("").folder, None);
    }
}
