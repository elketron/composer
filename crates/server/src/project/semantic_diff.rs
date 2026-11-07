//! The structural diff of one changed file, from `difft` (Difftastic) when it is installed.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

/// difft gets this long per file; a slower one falls back to the text diff.
const DIFFT_TIMEOUT: Duration = Duration::from_secs(10);
/// A file's structural diff beyond this is dropped (the text diff stays).
const SEMANTIC_MAX_BYTES: usize = 128 * 1024;

/// The `difft` executable on the PATH as it is now (looked up per call, not cached).
pub fn find_difft() -> Option<PathBuf> {
    find_in(&std::env::var_os("PATH")?)
}

fn find_in(path_var: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path_var)
        .map(|dir| dir.join("difft"))
        .find(|candidate| candidate.is_file())
}

/// `difft` (the executable at `difft`) on `file` as of `base` against its current content in
/// the checkout at `path`. `None` when difft doesn't know the file's language (it would only
/// diff it as text), has nothing to show, fails or times out.
#[allow(
    clippy::disallowed_methods,
    reason = "the git jail's own bwrap (or git, with the sandbox off)"
)]
pub async fn semantic_diff(
    difft: &Path,
    path: &Path,
    base: &str,
    file: &str,
    status: &str,
) -> Option<String> {
    let name = Path::new(file).file_name()?;
    let dir = tempfile::tempdir().ok()?;
    let (old, new) = (dir.path().join("old"), dir.path().join("new"));
    std::fs::create_dir_all(&old).ok()?;
    std::fs::create_dir_all(&new).ok()?;
    let (old, new) = (old.join(name), new.join(name));
    if status.starts_with('A') {
        std::fs::write(&old, "").ok()?;
    } else {
        let (program, jailed) =
            crate::sandbox::git_jailed(path, &["show", &format!("{base}:{file}")]).ok()?;
        let shown = Command::new(program).args(jailed).output().await.ok()?;
        if !shown.status.success() {
            return None;
        }
        std::fs::write(&old, shown.stdout).ok()?;
    }
    if status.starts_with('D') {
        std::fs::write(&new, "").ok()?;
    } else {
        // The file as the worktree has it, never what a symlink there points to (a run could
        // link `~/.ssh/id_ed25519`): composer reads it outside the sandbox.
        let changed = path.join(file);
        if std::fs::symlink_metadata(&changed)
            .ok()?
            .file_type()
            .is_symlink()
        {
            return None;
        }
        std::fs::copy(&changed, &new).ok()?;
    }
    run_difft(difft, &old, &new, DIFFT_TIMEOUT).await
}

#[allow(
    clippy::disallowed_methods,
    reason = "composer's own difft on files git gave it"
)]
async fn run_difft(difft: &Path, old: &Path, new: &Path, timeout: Duration) -> Option<String> {
    let run = Command::new(difft)
        .args(["--color", "never", "--display", "inline", "--context", "3"])
        .arg(old)
        .arg(new)
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(timeout, run).await.ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let header = text.lines().next()?;
    if header.trim() == "No changes."
        || header.ends_with("--- Text")
        || header.contains("Binary")
        || text.len() > SEMANTIC_MAX_BYTES
    {
        return None;
    }
    Some(text.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// A `difft` script in a fresh directory that runs `body`.
    fn fake_difft(body: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("difft");
        std::fs::write(&bin, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, bin)
    }

    async fn run(body: &str, timeout: Duration) -> Option<String> {
        let (dir, bin) = fake_difft(body);
        let (old, new) = (dir.path().join("old"), dir.path().join("new"));
        run_difft(&bin, &old, &new, timeout).await
    }

    const LONG: Duration = Duration::from_secs(10);

    #[tokio::test]
    async fn structural_output_is_kept() {
        let out = run("printf 'a.rs --- Rust\\n1 2 x\\n'", LONG).await;
        assert_eq!(out.as_deref(), Some("a.rs --- Rust\n1 2 x"));
    }

    #[tokio::test]
    async fn a_slow_difft_times_out() {
        assert_eq!(
            run("sleep 3; echo 'a.rs --- Rust'", Duration::from_millis(200)).await,
            None
        );
    }

    #[tokio::test]
    async fn oversized_output_is_dropped() {
        let body = format!(
            "echo 'a.rs --- Rust'; head -c {} /dev/zero | tr '\\0' x",
            SEMANTIC_MAX_BYTES + 10
        );
        assert_eq!(run(&body, LONG).await, None);
    }

    #[tokio::test]
    async fn a_text_header_is_dropped() {
        assert_eq!(run("printf 'a.txt --- Text\\n1 2 x\\n'", LONG).await, None);
    }

    #[tokio::test]
    async fn no_changes_on_success_is_none() {
        assert_eq!(run("echo 'No changes.'", LONG).await, None);
    }

    #[tokio::test]
    async fn a_failing_difft_is_none() {
        assert_eq!(run("echo 'a.rs --- Rust'; exit 1", LONG).await, None);
    }

    #[test]
    fn difft_is_looked_up_on_the_given_path() {
        let (dir, bin) = fake_difft("true");
        let empty = tempfile::tempdir().unwrap();
        let path = std::env::join_paths([empty.path(), dir.path()]).unwrap();
        assert_eq!(find_in(&path), Some(bin));
        assert_eq!(find_in(empty.path().as_os_str()), None);
    }
}
