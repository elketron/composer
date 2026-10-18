use std::path::{Path, PathBuf};

/// The composer home folder (`~/.composer`, or `$COMPOSER_HOME`).
///
/// ```text
/// config.toml
/// sites/            the web (agent-visible)
/// state/            sessions, token, approvals (never agent-visible)
/// ```
#[derive(Debug, Clone)]
pub struct Home {
    root: PathBuf,
}

impl Home {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `$COMPOSER_HOME`, else `~/.composer`.
    pub fn from_env() -> Self {
        if let Some(dir) = std::env::var_os("COMPOSER_HOME") {
            return Self::new(dir);
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self::new(home.join(".composer"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.toml")
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

    /// The local client token (`state/token`). Devices get their own tokens later (pairing).
    pub fn token_file(&self) -> PathBuf {
        self.state().join("token")
    }
}
