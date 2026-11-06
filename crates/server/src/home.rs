use std::path::{Path, PathBuf};

use crate::error::{Result, ServerError};
use crate::workspace_home::WorkspaceHome;
use crate::workspace_name::WorkspaceName;

/// The composer home folder (`~/.composer`, or `$COMPOSER_HOME`): what all workspaces share.
///
/// ```text
/// global/           config.toml (defaults every workspace starts from), user.md
/// sites/            the library: site definitions workspaces install from
/// templates/        app scaffold and sdk
/// state/            server-only: stock bases for the library and templates
/// workspaces/<n>/   each workspace (see WorkspaceHome)
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
        if let Some(dir) = std::env::var_os(WorkspaceHome::ENV_HOME) {
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

    pub fn global(&self) -> PathBuf {
        self.root.join("global")
    }

    pub fn global_config_file(&self) -> PathBuf {
        self.global().join("config.toml")
    }

    /// The site library (`sites/`).
    pub fn library(&self) -> PathBuf {
        self.root.join("sites")
    }

    pub fn templates(&self) -> PathBuf {
        self.root.join("templates")
    }

    pub fn state(&self) -> PathBuf {
        self.root.join("state")
    }

    /// The master token (`state/token`): lists every workspace (`/api/workspaces`), opens none.
    pub fn master_token_file(&self) -> PathBuf {
        self.state().join("token")
    }

    pub fn workspaces_dir(&self) -> PathBuf {
        self.root.join("workspaces")
    }

    /// The workspace `name` (whether or not it exists yet).
    pub fn workspace(&self, name: &WorkspaceName) -> WorkspaceHome {
        WorkspaceHome::in_home(
            &self.root,
            self.workspaces_dir().join(name.as_str()),
            name.clone(),
        )
    }

    /// The workspaces that exist, by name. Folders under `workspaces/` whose name isn't a valid
    /// workspace name are skipped (with a warning), never opened.
    pub fn workspace_names(&self) -> Result<Vec<WorkspaceName>> {
        let dir = self.workspaces_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(ServerError::io(&dir, e)),
        };
        let mut names = Vec::new();
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let file_name = entry.file_name();
            match WorkspaceName::new(&file_name.to_string_lossy()) {
                Ok(name) => names.push(name),
                Err(e) => tracing::warn!("skipping {}: {e}", entry.path().display()),
            }
        }
        names.sort();
        Ok(names)
    }

    /// A home from before workspaces: `config.toml` and `sites/` at the root, no `workspaces/`.
    /// `composer migrate` moves it into `workspaces/default/`.
    pub fn is_single_workspace(&self) -> bool {
        self.root.join("config.toml").is_file() && !self.workspaces_dir().exists()
    }
}
