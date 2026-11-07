//! Running git for worktrees and ticket branches (blocking; these are quick).

use std::path::Path;
use std::process::{Command, Output};

use crate::sandbox::git_jailed;

/// Runs git in `dir` (in the git jail); stdout trimmed, or stderr as the error.
pub fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git_with(dir, args, &[])
}

/// The same with extra environment variables.
pub fn git_with(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
    trimmed(git_output(dir, args, env)?)
}

/// [`git`] on the library (`<home>/sites`): composer's own repository, which no sandbox can
/// write, so it runs as it is (composer starts even where the sandbox can't).
pub fn library_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    trimmed(library_git_output(dir, args)?)
}

/// [`git_output`] on the library.
#[allow(
    clippy::disallowed_methods,
    reason = "the library: composer's own repository, nothing in a sandbox can write it"
)]
pub fn library_git_output(dir: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("running git: {e}"))
}

fn trimmed(output: Output) -> Result<String, String> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// git's whole output, whatever its exit status. It runs in the git jail: what a sandboxed run
/// planted in the repository (hooks, `core.fsmonitor`, filters) can't reach beyond it.
#[allow(
    clippy::disallowed_methods,
    reason = "the git jail's own bwrap (or git, with the sandbox off)"
)]
pub fn git_output(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<Output, String> {
    let (program, jailed) = git_jailed(dir, args)?;
    Command::new(program)
        .args(jailed)
        .envs(env.iter().copied())
        .output()
        .map_err(|e| format!("running git: {e}"))
}
