//! The structural diff of one changed file, from `difft` (Difftastic) when it is installed.

use std::path::Path;
use std::time::Duration;

use tokio::process::Command;

/// difft gets this long per file; a slower one falls back to the text diff.
const DIFFT_TIMEOUT: Duration = Duration::from_secs(10);
/// A file's structural diff beyond this is dropped (the text diff stays).
const SEMANTIC_MAX_BYTES: usize = 128 * 1024;

/// `difft` on `file` as of `base` against its current content in the checkout at `path`.
/// `None` when difft isn't on the PATH (looked up now, per call), doesn't know the file's
/// language (it would only diff it as text), fails or times out.
pub async fn semantic_diff(path: &Path, base: &str, file: &str, status: &str) -> Option<String> {
    let name = Path::new(file).file_name()?;
    let dir = tempfile::tempdir().ok()?;
    let (old, new) = (dir.path().join("old"), dir.path().join("new"));
    std::fs::create_dir_all(&old).ok()?;
    std::fs::create_dir_all(&new).ok()?;
    let (old, new) = (old.join(name), new.join(name));
    if status.starts_with('A') {
        std::fs::write(&old, "").ok()?;
    } else {
        let shown = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["show", &format!("{base}:{file}")])
            .output()
            .await
            .ok()?;
        if !shown.status.success() {
            return None;
        }
        std::fs::write(&old, shown.stdout).ok()?;
    }
    if status.starts_with('D') {
        std::fs::write(&new, "").ok()?;
    } else {
        std::fs::copy(path.join(file), &new).ok()?;
    }
    let run = Command::new("difft")
        .args(["--color", "never", "--display", "inline", "--context", "3"])
        .arg(&old)
        .arg(&new)
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(DIFFT_TIMEOUT, run).await.ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let header = text.lines().next()?;
    if header.ends_with("--- Text") || header.contains("Binary") || text.len() > SEMANTIC_MAX_BYTES
    {
        return None;
    }
    Some(text.trim_end().to_string())
}
