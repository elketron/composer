use std::fs;
use std::path::Path;

use include_dir::{Dir, include_dir};

use crate::config::Config;
use crate::error::{Result, ServerError};
use crate::home::Home;

static SEED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../seed/sites");

/// Creates the home folder: seed sites, default config, state folders, local token.
/// Never overwrites existing files. Returns the paths it created.
pub fn init_home(home: &Home) -> Result<Vec<String>> {
    let mut created = Vec::new();
    for dir in [home.sites(), home.sessions()] {
        fs::create_dir_all(&dir).map_err(|e| ServerError::io(&dir, e))?;
    }
    copy_seed(&SEED, &home.sites(), &mut created)?;
    write_new(
        &home.config_file(),
        &Config::defaults().to_toml(),
        &mut created,
    )?;
    write_new(&home.token_file(), &new_token(), &mut created)?;
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
