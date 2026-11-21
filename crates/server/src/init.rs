use std::fs;
use std::path::Path;

use include_dir::{Dir, include_dir};

use crate::actions::{Approvals, Justfile};
use crate::config::Config;
use crate::error::{Result, ServerError};
use crate::home::Home;

static SEED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../seed/sites");

/// Creates the composer home: its config, the stock sites, state folders and token. Never
/// overwrites existing files: the stock is seeded once, never merged later. Recipes of newly
/// created stock justfiles are pre-approved (needs `just`; otherwise they are approved on first
/// use). Returns the paths it created.
pub async fn init_home(home: &Home) -> Result<Vec<String>> {
    let mut created = Vec::new();
    for dir in [home.sites(), home.sessions()] {
        fs::create_dir_all(&dir).map_err(|e| ServerError::io(&dir, e))?;
    }
    copy_seed(&SEED, &home.sites(), &mut created)?;
    write_new(&home.config_file(), Config::TEMPLATE, &mut created)?;
    write_new(&home.token_file(), &new_token(), &mut created)?;
    approve_recipes_in(&Approvals::load(&home.approvals_file())?, &created).await?;
    Ok(created)
}

/// Reads the home's token, creating it if needed.
pub fn local_token(home: &Home) -> Result<String> {
    read_or_create_token(&home.token_file())
}

/// Reads a token file, creating it (and its folder) with a new token if it doesn't exist.
pub(crate) fn read_or_create_token(path: &Path) -> Result<String> {
    if !path.exists() {
        write_creating_dirs(path, new_token().as_bytes())?;
    }
    fs::read_to_string(path)
        .map(|t| t.trim().to_string())
        .map_err(|e| ServerError::io(path, e))
}

/// Pre-approves the recipes of the stock justfiles among `created`.
async fn approve_recipes_in(approvals: &Approvals, created: &[String]) -> Result<()> {
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
            approvals.approve(site, &recipe.name, &justfile.hash(&recipe.name))?;
        }
    }
    Ok(())
}

/// Writes the files of `dir` that don't exist under `root` yet.
fn copy_seed(dir: &Dir<'_>, root: &Path, created: &mut Vec<String>) -> Result<()> {
    for file in dir.files() {
        let dest = root.join(file.path());
        if !dest.exists() {
            write_creating_dirs(&dest, file.contents())?;
            created.push(dest.display().to_string());
        }
    }
    for sub in dir.dirs() {
        copy_seed(sub, root, created)?;
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
