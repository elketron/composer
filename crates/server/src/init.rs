use std::fs;
use std::path::Path;

use include_dir::{Dir, include_dir};

use crate::actions::{Approvals, Caller, Justfile};
use crate::config::Config;
use crate::error::{Result, ServerError};
use crate::home::Home;
use crate::stock_root::StockRoot;
use crate::workspace_home::WorkspaceHome;
use crate::workspace_name::WorkspaceName;

static SEED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../seed/sites");
static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../templates");

/// What a new workspace's `config.toml` says: everything comes from the global config.
const WORKSPACE_CONFIG: &str = "\
# This workspace's settings. Everything in the home's global/config.toml applies here; set a key
# in this file to override it for this workspace only (tables merge key by key).
# Usually per workspace: [projects], [mcp.<name>], [[launcher]], [claude_code].
";

/// The stock folders this binary carries: (name under the root and `state/seed/`, contents).
pub(crate) fn stock() -> [(&'static str, &'static Dir<'static>); 2] {
    [("sites", &SEED), ("templates", &TEMPLATES)]
}

/// Creates the composer home: global config, the site library and templates, server state, and
/// the `default` workspace if there is no workspace yet. Never overwrites existing files.
/// Returns the paths it created.
pub async fn init_home(home: &Home) -> Result<Vec<String>> {
    let mut created = Vec::new();
    for dir in [home.global(), home.state(), home.workspaces_dir()] {
        fs::create_dir_all(&dir).map_err(|e| ServerError::io(&dir, e))?;
    }
    write_new(&home.global_config_file(), Config::TEMPLATE, &mut created)?;
    install(&StockRoot::library(home), &mut created)?;
    if home.workspace_names()?.is_empty() {
        let default = home.workspace(&WorkspaceName::default_workspace());
        created.extend(init_workspace(&default).await?);
    }
    Ok(created)
}

/// Creates a workspace: its copy of the stock sites, its config, state folders and token. Never
/// overwrites existing files. Recipes of newly created stock justfiles are pre-approved (needs
/// `just`; otherwise they are approved on first use). A standalone workspace (its own home, as
/// in tests) also gets the templates and the full config template.
pub async fn init_workspace(workspace: &WorkspaceHome) -> Result<Vec<String>> {
    let mut created = Vec::new();
    for dir in [workspace.sites(), workspace.sessions()] {
        fs::create_dir_all(&dir).map_err(|e| ServerError::io(&dir, e))?;
    }
    let stock = StockRoot::workspace(workspace);
    install(&stock, &mut created)?;
    let config = if workspace.is_standalone() {
        Config::TEMPLATE
    } else {
        WORKSPACE_CONFIG
    };
    write_new(&workspace.config_file(), config, &mut created)?;
    write_new(&workspace.token_file(), &new_token(), &mut created)?;
    if let Some(approvals) = stock.approvals() {
        approve_seed_recipes(approvals, &created).await?;
    }
    Ok(created)
}

/// Reads the workspace's token, creating it if needed.
pub fn local_token(workspace: &WorkspaceHome) -> Result<String> {
    let path = workspace.token_file();
    if !path.exists() {
        let state = workspace.state();
        fs::create_dir_all(&state).map_err(|e| ServerError::io(&state, e))?;
        fs::write(&path, new_token()).map_err(|e| ServerError::io(&path, e))?;
    }
    fs::read_to_string(&path)
        .map(|t| t.trim().to_string())
        .map_err(|e| ServerError::io(&path, e))
}

/// Pre-approves the recipes of the stock justfiles among `paths` (as `init` and `update` install
/// them), for agents and apps.
pub(crate) async fn approve_seed_recipes(approvals: &Path, created: &[String]) -> Result<()> {
    let approvals = Approvals::load(approvals)?;
    for path in created.iter().filter(|p| p.ends_with("/justfile")) {
        let Some(site_dir) = Path::new(path).parent() else {
            continue;
        };
        let Some(site) = site_dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Ok(justfile) = Justfile::load(site_dir).await else {
            continue;
        };
        for recipe in justfile.recipes() {
            let hash = justfile.hash(&recipe.name);
            approvals.approve(site, &recipe.name, &hash, Caller::Agent)?;
            approvals.approve(site, &recipe.name, &hash, Caller::App)?;
        }
    }
    Ok(())
}

/// Installs every stock tree of `stock` that isn't there yet.
fn install(stock: &StockRoot, created: &mut Vec<String>) -> Result<()> {
    for (name, dir) in stock.trees() {
        copy_seed(dir, stock, name, created)?;
    }
    Ok(())
}

/// Writes the files that don't exist yet, recording each as the base for `update`.
fn copy_seed(
    dir: &Dir<'_>,
    stock: &StockRoot,
    name: &str,
    created: &mut Vec<String>,
) -> Result<()> {
    for file in dir.files() {
        let dest = stock.root().join(name).join(file.path());
        if !dest.exists() {
            write_creating_dirs(&dest, file.contents())?;
            write_creating_dirs(
                &stock.seed_base().join(name).join(file.path()),
                file.contents(),
            )?;
            created.push(dest.display().to_string());
        }
    }
    for sub in dir.dirs() {
        copy_seed(sub, stock, name, created)?;
    }
    Ok(())
}

pub(crate) fn write_creating_dirs(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| ServerError::io(parent, e))?;
    }
    fs::write(path, content).map_err(|e| ServerError::io(path, e))
}

fn write_new(path: &Path, content: &str, created: &mut Vec<String>) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    write_creating_dirs(path, content.as_bytes())?;
    created.push(path.display().to_string());
    Ok(())
}

pub(crate) fn new_token() -> String {
    (0..4)
        .map(|_| format!("{:016x}", rand::random::<u64>()))
        .collect()
}
