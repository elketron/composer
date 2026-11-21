use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::Result;
use crate::mcp_bundles::InstalledMcpBundles;

/// The composer home folder (`~/.composer`, or `$COMPOSER_HOME`): one home, one web.
///
/// ```text
/// config.toml       the settings
/// sites/            the web (agent-visible)
/// mcps/             `.mcpb` bundles: putting a file there installs it
/// state/            sessions, token, approvals, unpacked bundles (never agent-visible)
/// ```
#[derive(Debug, Clone)]
pub struct Home {
    root: PathBuf,
}

impl Home {
    /// Child processes (the MCP bridge) find their home through it.
    pub const ENV_HOME: &'static str = "COMPOSER_HOME";

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `$COMPOSER_HOME`, else `~/.composer`.
    pub fn from_env() -> Self {
        if let Some(dir) = std::env::var_os(Self::ENV_HOME) {
            return Self::new(dir);
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self::new(home.join(".composer"))
    }

    /// The environment that makes a child process open this same home.
    pub fn env(&self) -> [(&'static str, String); 1] {
        [(Self::ENV_HOME, self.root.display().to_string())]
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    /// The settings, with the installed MCP bundles' servers.
    pub fn load_config(&self) -> Result<Config> {
        let mut config = Config::load(&self.config_file())?;
        InstalledMcpBundles::load(&self.mcps(), &self.mcps_state()).merge_into(&mut config.mcp);
        Ok(config)
    }

    /// The MCP bundles (`mcps/*.mcpb`).
    pub fn mcps(&self) -> PathBuf {
        self.root.join("mcps")
    }

    /// Where the bundles are unpacked (`state/mcps/<server>/`).
    pub fn mcps_state(&self) -> PathBuf {
        self.state().join("mcps")
    }

    pub fn sites(&self) -> PathBuf {
        self.root.join("sites")
    }

    pub fn state(&self) -> PathBuf {
        self.root.join("state")
    }

    pub fn sessions(&self) -> PathBuf {
        self.state().join("sessions")
    }

    /// Approved recipe versions.
    pub fn approvals_file(&self) -> PathBuf {
        self.state().join("approvals.json")
    }

    /// The client token (`state/token`). Devices get their own tokens (pairing).
    pub fn token_file(&self) -> PathBuf {
        self.state().join("token")
    }
}
