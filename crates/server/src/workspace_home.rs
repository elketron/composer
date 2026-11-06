use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::Result;
use crate::workspace_name::WorkspaceName;

/// One workspace's folder: its web, config and state.
///
/// ```text
/// config.toml       this workspace's settings, on top of the home's global/config.toml
/// sites/            the web (agent-visible)
/// state/            sessions, token, approvals, stock file bases (never agent-visible)
/// ```
///
/// Normally it lives at `<home>/workspaces/<name>/` (see [`crate::Home::workspace`]) and shares
/// the home's templates and global config. [`WorkspaceHome::new`] makes a standalone one, where
/// the folder is home and workspace at once (`templates/` inside it): tests use that.
#[derive(Debug, Clone)]
pub struct WorkspaceHome {
    root: PathBuf,
    /// The composer home this workspace belongs to (the same folder when standalone).
    home: PathBuf,
    name: WorkspaceName,
}

impl WorkspaceHome {
    /// Child processes (workers, the MCP bridge) find their workspace through these.
    pub const ENV_HOME: &'static str = "COMPOSER_HOME";
    pub const ENV_NAME: &'static str = "COMPOSER_WORKSPACE";
    pub const ENV_ROOT: &'static str = "COMPOSER_WORKSPACE_HOME";

    /// A standalone workspace: `root` is both the home and the workspace (named `default`).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            home: root.clone(),
            root,
            name: WorkspaceName::default_workspace(),
        }
    }

    /// The workspace `name` at `root` inside the composer home `home`.
    pub fn in_home(
        home: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        name: WorkspaceName,
    ) -> Self {
        Self {
            root: root.into(),
            home: home.into(),
            name,
        }
    }

    /// The workspace a child process was started for ([`WorkspaceHome::env`]), if it was.
    pub fn from_env() -> Result<Option<Self>> {
        let Some(root) = std::env::var_os(Self::ENV_ROOT) else {
            return Ok(None);
        };
        let home = std::env::var_os(Self::ENV_HOME).unwrap_or_else(|| root.clone());
        let name = match std::env::var(Self::ENV_NAME) {
            Ok(name) => WorkspaceName::new(&name)?,
            Err(_) => WorkspaceName::default_workspace(),
        };
        Ok(Some(Self::in_home(home, root, name)))
    }

    /// The environment that makes a child process open this same workspace.
    pub fn env(&self) -> [(&'static str, String); 3] {
        [
            (Self::ENV_HOME, self.home.display().to_string()),
            (Self::ENV_NAME, self.name.to_string()),
            (Self::ENV_ROOT, self.root.display().to_string()),
        ]
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(&self) -> &WorkspaceName {
        &self.name
    }

    /// The composer home folder it belongs to.
    pub fn home_root(&self) -> &Path {
        &self.home
    }

    /// Standalone: home and workspace are one folder (tests, and old single-workspace homes).
    pub fn is_standalone(&self) -> bool {
        self.home == self.root
    }

    /// Whose part of the composer home `path` lies in or holds, when that is not this workspace's
    /// and not a shared part (the library `sites/`, `global/`, `templates/`): another workspace's
    /// folder, the home's own `state/` (master token, project owners), or a folder holding every
    /// workspace. What an agent of this workspace may never work in or link to. `None` for paths
    /// outside the home, and always for a standalone home.
    pub fn foreign_part(&self, path: &Path) -> Option<String> {
        if self.is_standalone() {
            return None;
        }
        let real = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let (path, own, home) = (real(path), real(&self.root), real(&self.home));
        if path.starts_with(&own) {
            return None;
        }
        if own.starts_with(&path) {
            return Some(format!(
                "{} holds every workspace of the composer home",
                path.display()
            ));
        }
        let rel = path.strip_prefix(&home).ok()?;
        let mut parts = rel.components().map(|c| c.as_os_str().to_string_lossy());
        match (parts.next().as_deref(), parts.next()) {
            (Some("sites" | "global" | "templates"), _) => None,
            (Some("workspaces"), Some(other)) => Some(format!(
                "{} is in workspace `{other}`'s folder",
                path.display()
            )),
            _ => Some(format!(
                "{} is the composer home's own state",
                path.display()
            )),
        }
    }

    /// The home's index of which workspace owns which project.
    pub fn projects_file(&self) -> PathBuf {
        crate::Home::new(&self.home).projects_file()
    }

    /// Where the host mounts this workspace's API: `/w/<name>`, or `""` when standalone (served
    /// alone, as tests do).
    pub fn route_base(&self) -> String {
        if self.is_standalone() {
            String::new()
        } else {
            format!("/w/{}", self.name)
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    /// The home's defaults this workspace's config goes on top of.
    pub fn global_config_file(&self) -> PathBuf {
        self.home.join("global").join("config.toml")
    }

    /// This workspace's settings: the global defaults with its own `config.toml` on top.
    pub fn load_config(&self) -> Result<Config> {
        Config::load_layered(&self.global_config_file(), &self.config_file())
    }

    pub fn sites(&self) -> PathBuf {
        self.root.join("sites")
    }

    /// App scaffold (`app/`) and the shared sdk (`sdk/`): the home's, shared by its workspaces.
    pub fn templates(&self) -> PathBuf {
        self.home.join("templates")
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

    /// This workspace's client token (`state/token`). Devices get their own tokens (pairing).
    pub fn token_file(&self) -> PathBuf {
        self.state().join("token")
    }
}
