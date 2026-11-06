use std::path::{Path, PathBuf};

use include_dir::Dir;

use crate::home::Home;
use crate::init::stock;
use crate::workspace_home::WorkspaceHome;

/// A folder holding stock trees (`sites/`, `templates/`) that `init` installs and `update` keeps
/// up to date, with its own bases (`state/seed/`) and review copies (`state/updates/`).
///
/// Two kinds exist: the home's library (`sites/` and `templates/` at the home root) and each
/// workspace (its own copy of the stock `sites/`). A standalone workspace carries `templates/`
/// too, since it is its own home.
pub(crate) struct StockRoot {
    root: PathBuf,
    state: PathBuf,
    trees: Vec<(&'static str, &'static Dir<'static>)>,
    /// Where recipes of installed justfiles are pre-approved: a workspace's approvals. The
    /// library's recipes never run, so it has none.
    approvals: Option<PathBuf>,
    /// Prefix of reported paths, relative to the composer home (`""` or `workspaces/<n>/`).
    label: String,
}

impl StockRoot {
    /// The home's library and templates.
    pub fn library(home: &Home) -> Self {
        Self {
            root: home.root().to_path_buf(),
            state: home.state(),
            trees: stock().to_vec(),
            approvals: None,
            label: String::new(),
        }
    }

    /// A workspace's stock sites (and templates, when standalone).
    pub fn workspace(workspace: &WorkspaceHome) -> Self {
        let standalone = workspace.is_standalone();
        let trees = stock()
            .into_iter()
            .filter(|(name, _)| standalone || *name == "sites")
            .collect();
        let label = if standalone {
            String::new()
        } else {
            format!("workspaces/{}/", workspace.name())
        };
        Self {
            root: workspace.root().to_path_buf(),
            state: workspace.state(),
            trees,
            approvals: Some(workspace.approvals_file()),
            label,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn trees(&self) -> &[(&'static str, &'static Dir<'static>)] {
        &self.trees
    }

    pub fn approvals(&self) -> Option<&Path> {
        self.approvals.as_deref()
    }

    /// The stock files as last installed: the base `update` merges from.
    pub fn seed_base(&self) -> PathBuf {
        self.state.join("seed")
    }

    /// New stock versions `update` could not apply, for the user to look at.
    pub fn updates(&self) -> PathBuf {
        self.state.join("updates")
    }

    /// Scratch space for three-way merges.
    pub fn merge_scratch(&self) -> PathBuf {
        self.state.join("merge")
    }

    /// `path` (relative to this root) as reported, relative to the composer home.
    pub fn label(&self, path: &str) -> String {
        format!("{}{path}", self.label)
    }
}
