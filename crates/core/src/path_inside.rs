use std::path::{Component, Path, PathBuf};

/// `path` inside `folder` (`what` names it in the refusal): relative, no `..`, and no symlink on
/// the way that leads out. What an app (its project files, its data) and an agent reading app data
/// (`data://`) may reach.
pub fn path_inside(folder: &Path, path: &str, what: &str) -> Result<PathBuf, String> {
    let relative = Path::new(path);
    let plain = relative
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    let outside =
        || format!("{path} isn't inside {what}: give a path relative to it, without `..`");
    if !plain || path.is_empty() {
        return Err(outside());
    }
    let root = std::fs::canonicalize(folder).map_err(|_| outside())?;
    let file = root.join(relative);
    // The deepest part that exists, symlinks resolved, must still be in the folder.
    let existing = file
        .ancestors()
        .find(|a| a.exists())
        .and_then(|a| std::fs::canonicalize(a).ok())
        .ok_or_else(outside)?;
    if !existing.starts_with(&root) {
        return Err(outside());
    }
    Ok(file)
}
