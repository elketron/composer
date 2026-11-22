use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config_sections::{
    AgentConfig, AppConfig, AttachmentsConfig, DesktopConfig, McpServerConfig, ProviderConfig,
    ServerConfig, System1Config, TerminalsConfig, ViewerConfig,
};
use crate::error::{Result, ServerError};

/// `config.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "section")]
    pub provider: ProviderConfig,
    #[serde(default = "section")]
    pub server: ServerConfig,
    #[serde(default = "section")]
    pub system1: System1Config,
    #[serde(default = "section")]
    pub agent: AgentConfig,
    #[serde(default = "section")]
    pub attachments: AttachmentsConfig,
    #[serde(default = "section")]
    pub terminals: TerminalsConfig,
    #[serde(default = "section")]
    pub desktop: DesktopConfig,
    #[serde(default = "section")]
    pub viewer: ViewerConfig,
    /// MCP servers by name, `[mcp.github]`.
    #[serde(default)]
    pub mcp: BTreeMap<String, McpServerConfig>,
    /// Webhooks by name, `[hooks.krea]`: `POST /api/hooks/krea`.
    #[serde(default)]
    pub hooks: BTreeMap<String, crate::runs::HookConfig>,
    /// Outside agents over the Agent Client Protocol by name, `[acp.claude]`: a session's
    /// `harness`.
    #[serde(default)]
    pub acp: BTreeMap<String, crate::harness::AcpConfig>,
    /// The user's own local web servers by name, `[apps.canvas]`: served at `/apps/canvas/`.
    #[serde(default)]
    pub apps: BTreeMap<String, AppConfig>,
}

impl Config {
    /// What `composer init` writes: every setting at its default, with what it does.
    pub const TEMPLATE: &str = include_str!("config_template.toml");

    /// All defaults.
    pub fn defaults() -> Self {
        Self::parse("").expect("empty config is valid")
    }

    pub fn parse(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text).map_err(|e| ServerError::Config(e.to_string()))?;
        crate::app_upstream::check(&config.apps).map_err(ServerError::Config)?;
        Ok(config)
    }

    /// Reads the file; a missing file gives the defaults.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::defaults()),
            Err(e) => Err(ServerError::io(path, e)),
        }
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("config serializes")
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
        assert!(c.system1.model.is_empty());
        assert_eq!(c.system1.timeout_secs, 15);
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
    fn a_config_from_before_the_sandbox_was_removed_still_loads() {
        let c = Config::parse(
            "[sandbox]\nenabled = true\nsystem_dirs = [\"/opt\"]\nbinds_rw = [\"~/Projects\"]\n\n\
             [terminals]\nscrollback_kb = 3\nlend = [\"~/.zshrc\"]\n\n\
             [mcp.blender]\ncommand = \"blender-mcp\"\nbinds_ro = [\"/ro\"]\nbinds_rw = []\n",
        )
        .unwrap();
        assert_eq!(c.terminals.scrollback_kb, 3);
        assert_eq!(c.mcp["blender"].command, "blender-mcp");
        let saved = c.to_toml();
        for gone in ["[sandbox]", "lend =", "binds_ro", "binds_rw"] {
            assert!(!saved.contains(gone), "{gone} in {saved}");
        }
    }
}
