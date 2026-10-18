use std::fs;
use std::path::Path;

use include_dir::{Dir, include_dir};

use crate::actions::{Approvals, Justfile};
use crate::config::Config;
use crate::error::{Result, ServerError};
use crate::home::Home;

static SEED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../seed/sites");
static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../templates");

/// Creates the home folder: seed sites, app templates, default config, state folders, local token.
/// Never overwrites existing files. Recipes of newly created seed justfiles are
/// pre-approved (needs `just`; otherwise they are approved on first use).
/// Returns the paths it created.
pub async fn init_home(home: &Home) -> Result<Vec<String>> {
    let mut created = Vec::new();
    for dir in [home.sites(), home.sessions()] {
        fs::create_dir_all(&dir).map_err(|e| ServerError::io(&dir, e))?;
    }
    copy_seed(&SEED, &home.sites(), &mut created)?;
    copy_seed(&TEMPLATES, &home.templates(), &mut created)?;
    write_new(
        &home.config_file(),
        &Config::defaults().to_toml(),
        &mut created,
    )?;
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

async fn approve_seed_recipes(home: &Home, created: &[String]) -> Result<()> {
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

fn copy_seed(dir: &Dir<'_>, dest_root: &Path, created: &mut Vec<String>) -> Result<()> {
    for file in dir.files() {
        let dest = dest_root.join(file.path());
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| ServerError::io(parent, e))?;
        }
        if !dest.exists() {
            fs::write(&dest, file.contents()).map_err(|e| ServerError::io(&dest, e))?;
            created.push(dest.display().to_string());
        }
    }
    for sub in dir.dirs() {
        copy_seed(sub, dest_root, created)?;
    }
    Ok(())
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
