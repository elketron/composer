use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config_sections::{
    AgentConfig, ApplicationConfig, AppsConfig, AttachmentsConfig, ClaudeCodeConfig, DesktopConfig,
    LauncherEntry, McpServerConfig, ProjectsConfig, ProviderConfig, RemindersConfig, SandboxConfig,
    ServerConfig, System1Config, TerminalsConfig, ViewerConfig,
};
use crate::error::{Result, ServerError};

/// `config.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "section")]
    pub provider: ProviderConfig,
    #[serde(default = "section")]
    pub system1: System1Config,
    #[serde(default = "section")]
    pub server: ServerConfig,
    #[serde(default = "section")]
    pub agent: AgentConfig,
    #[serde(default = "section")]
    pub projects: ProjectsConfig,
    #[serde(default = "section")]
    pub reminders: RemindersConfig,
    #[serde(default = "section")]
    pub claude_code: ClaudeCodeConfig,
    #[serde(default = "section")]
    pub attachments: AttachmentsConfig,
    #[serde(default = "section")]
    pub sandbox: SandboxConfig,
    #[serde(default = "section")]
    pub apps: AppsConfig,
    #[serde(default = "section")]
    pub terminals: TerminalsConfig,
    #[serde(default = "section")]
    pub desktop: DesktopConfig,
    #[serde(default = "section")]
    pub viewer: ViewerConfig,
    /// MCP servers by name, `[mcp.github]`.
    #[serde(default)]
    pub mcp: BTreeMap<String, McpServerConfig>,
    /// Launcher entries, `[[launcher]]`: apps the web's apps can open on the desktop.
    #[serde(default)]
    pub launcher: Vec<LauncherEntry>,
    /// Applications a site's `app:` starts on entering, `[applications.blender]`.
    #[serde(default)]
    pub applications: BTreeMap<String, ApplicationConfig>,
}

impl Config {
    /// What `composer init` writes: every setting at its default, with what it does.
    pub const TEMPLATE: &str = include_str!("config_template.toml");

    /// All defaults.
    pub fn defaults() -> Self {
        Self::parse("").expect("empty config is valid")
    }

    pub fn parse(text: &str) -> Result<Self> {
        toml::from_str(text).map_err(|e| ServerError::Config(e.to_string()))
    }

    /// Reads the file; a missing file gives the defaults.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::defaults()),
            Err(e) => Err(ServerError::io(path, e)),
        }
    }

    /// A workspace's config: `global` (the home's defaults) with `workspace` on top. Tables merge
    /// key by key, at any depth; any other value in `workspace` replaces the global one. Either
    /// file may be missing.
    pub fn load_layered(global: &Path, workspace: &Path) -> Result<Self> {
        let mut merged = read_table(global)?;
        merge_into(&mut merged, read_table(workspace)?);
        merged
            .try_into()
            .map_err(|e: toml::de::Error| ServerError::Config(e.to_string()))
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("config serializes")
    }
}

/// The file as a TOML table; a missing file is an empty one.
fn read_table(path: &Path) -> Result<toml::Table> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(toml::Table::new()),
        Err(e) => return Err(ServerError::io(path, e)),
    };
    text.parse::<toml::Table>()
        .map_err(|e| ServerError::Config(format!("{}: {e}", path.display())))
}

/// `over` on top of `base`: tables merge key by key, everything else replaces.
fn merge_into(base: &mut toml::Table, over: toml::Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(base_table)), toml::Value::Table(over_table)) => {
                merge_into(base_table, over_table)
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// A section with all its field defaults.
fn section<T: serde::de::DeserializeOwned>() -> T {
    toml::from_str("").expect("every config field has a default")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_defaults() {
        let c = Config::defaults();
        assert_eq!(c.agent.slots, 2);
        assert_eq!(c.server.bind, "127.0.0.1:7420");
        assert!(c.provider.model.is_empty());
    }

    #[test]
    fn template_is_the_defaults() {
        assert_eq!(Config::parse(Config::TEMPLATE).unwrap(), Config::defaults());
    }

    #[test]
    fn partial_sections() {
        let c = Config::parse("[agent]\nslots = 1\n").unwrap();
        assert_eq!(c.agent.slots, 1);
        assert_eq!(c.agent.turn_tokens, 200_000);
        assert_eq!(c.agent.max_steps, 0);
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
    }

    #[test]
    fn launcher_entries() {
        let c = Config::parse(
            "[[launcher]]\nname = \"blender\"\ncommand = \"blender\"\nfolder = \"app/blockouts\"\nargs = [\"--background\"]\n\n[[launcher]]\nname = \"viewer\"\ncommand = \"xdg-open\"\nfolder = \"app\"\n",
        )
        .unwrap();
        assert_eq!(c.launcher.len(), 2);
        assert_eq!(c.launcher[0].args, ["--background"]);
        assert!(c.launcher[1].args.is_empty());
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
    }
    #[test]
    fn workspace_config_goes_on_top_of_global() {
        let dir = tempfile::tempdir().unwrap();
        let global = dir.path().join("global.toml");
        let workspace = dir.path().join("workspace.toml");
        std::fs::write(
            &global,
            "[provider]\nbase_url = \"http://g\"\nmodel = \"m\"\n\n[agent]\nslots = 3\n\n[projects.\"~/a\"]\nexternal_harnesses = [\"claude-code\"]\n",
        )
        .unwrap();
        std::fs::write(
            &workspace,
            "[provider]\nmodel = \"w\"\n\n[projects.\"~/b\"]\nexternal_harnesses = []\n",
        )
        .unwrap();
        let c = Config::load_layered(&global, &workspace).unwrap();
        assert_eq!(c.provider.base_url, "http://g", "kept from global");
        assert_eq!(c.provider.model, "w", "workspace wins");
        assert_eq!(c.agent.slots, 3);
        assert!(c.projects.settings.contains_key("~/a"), "tables merge");
        assert!(c.projects.settings.contains_key("~/b"));
        // Both missing: the defaults.
        let none = dir.path().join("nope.toml");
        assert_eq!(
            Config::load_layered(&none, &none).unwrap(),
            Config::defaults()
        );
    }

    #[test]
    fn per_project_settings() {
        let c = Config::parse(
            "[projects]\ndir = \"~/code\"\n\n[projects.\"~/code/app\"]\nexternal_harnesses = [\"claude-code\"]\n",
        )
        .unwrap();
        assert_eq!(c.projects.dir, "~/code");
        assert_eq!(
            c.projects.settings["~/code/app"].external_harnesses,
            ["claude-code"]
        );
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
    }
}
