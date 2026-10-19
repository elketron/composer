use std::path::{Path, PathBuf};

/// The composer home folder (`~/.composer`, or `$COMPOSER_HOME`).
///
/// ```text
/// config.toml
/// sites/            the web (agent-visible)
/// state/            sessions, token, approvals, stock file bases (never agent-visible)
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

    /// App scaffold (`app/`) and the shared sdk (`sdk/`).
    pub fn templates(&self) -> PathBuf {
        self.root.join("templates")
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

    /// The stock files as last installed by `init` or `update`: the base `update` merges from.
    pub fn seed_base(&self) -> PathBuf {
        self.state().join("seed")
    }

    /// New stock versions `update` could not apply, for the user to look at.
    pub fn updates(&self) -> PathBuf {
        self.state().join("updates")
    }

    /// The local client token (`state/token`). Devices get their own tokens later (pairing).
    pub fn token_file(&self) -> PathBuf {
        self.state().join("token")
    }
}
