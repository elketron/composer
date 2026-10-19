use std::fs;
use std::path::Path;

use include_dir::{Dir, include_dir};

use crate::actions::{Approvals, Justfile};
use crate::config::Config;
use crate::error::{Result, ServerError};
use crate::home::Home;

static SEED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../seed/sites");
static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../templates");

/// The stock folders this binary carries: (name under the home and `state/seed/`, contents).
pub(crate) fn stock() -> [(&'static str, &'static Dir<'static>); 2] {
    [("sites", &SEED), ("templates", &TEMPLATES)]
}

/// Creates the home folder: seed sites, app templates, default config, state folders, local token.
/// Never overwrites existing files. Recipes of newly created seed justfiles are
/// pre-approved (needs `just`; otherwise they are approved on first use).
/// Returns the paths it created.
pub async fn init_home(home: &Home) -> Result<Vec<String>> {
    let mut created = Vec::new();
    for dir in [home.sites(), home.sessions()] {
        fs::create_dir_all(&dir).map_err(|e| ServerError::io(&dir, e))?;
    }
    for (name, dir) in stock() {
        copy_seed(dir, home, name, &mut created)?;
    }
    write_new(&home.config_file(), Config::TEMPLATE, &mut created)?;
    write_new(&home.token_file(), &new_token(), &mut created)?;
    approve_seed_recipes(home, &created).await?;
    Ok(created)
}

/// Reads the local token, creating it if needed.
pub fn local_token(home: &Home) -> Result<String> {
    let path = home.token_file();
    if !path.exists() {
        fs::create_dir_all(home.state()).map_err(|e| ServerError::io(home.state(), e))?;
        fs::write(&path, new_token()).map_err(|e| ServerError::io(&path, e))?;
    }
    fs::read_to_string(&path)
        .map(|t| t.trim().to_string())
        .map_err(|e| ServerError::io(&path, e))
}

/// Pre-approves the recipes of the stock justfiles among `paths` (as `init` and `update` install them).
pub(crate) async fn approve_seed_recipes(home: &Home, created: &[String]) -> Result<()> {
    let approvals = Approvals::load(&home.approvals_file())?;
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

/// Writes the files that don't exist yet, recording each as the base for `update`.
fn copy_seed(dir: &Dir<'_>, home: &Home, name: &str, created: &mut Vec<String>) -> Result<()> {
    for file in dir.files() {
        let dest = home.root().join(name).join(file.path());
        if !dest.exists() {
            write_creating_dirs(&dest, file.contents())?;
            write_creating_dirs(
                &home.seed_base().join(name).join(file.path()),
                file.contents(),
            )?;
            created.push(dest.display().to_string());
        }
    }
    for sub in dir.dirs() {
        copy_seed(sub, home, name, created)?;
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
    fs::write(path, content).map_err(|e| ServerError::io(path, e))?;
    created.push(path.display().to_string());
    Ok(())
}

fn new_token() -> String {
    (0..4)
        .map(|_| format!("{:016x}", rand::random::<u64>()))
        .collect()
}
