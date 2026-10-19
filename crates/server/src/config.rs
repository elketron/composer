use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config_sections::{AgentConfig, ProjectsConfig, ProviderConfig, ServerConfig};
use crate::error::{Result, ServerError};

/// `config.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "section")]
    pub provider: ProviderConfig,
    #[serde(default = "section")]
    pub server: ServerConfig,
    #[serde(default = "section")]
    pub agent: AgentConfig,
    #[serde(default = "section")]
    pub projects: ProjectsConfig,
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
    }

    #[test]
    fn template_is_the_defaults() {
        assert_eq!(Config::parse(Config::TEMPLATE).unwrap(), Config::defaults());
    }

    #[test]
    fn partial_sections() {
        let c = Config::parse("[agent]\nslots = 1\n").unwrap();
        assert_eq!(c.agent.slots, 1);
        assert_eq!(c.agent.max_steps, 40);
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
    }
}
