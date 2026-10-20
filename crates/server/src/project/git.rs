//! Running git for worktrees and ticket branches (blocking; these are quick).

use std::path::Path;
use std::process::{Command, Output};

/// Runs git in `dir`; stdout trimmed, or stderr as the error.
pub fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git_with(dir, args, &[])
}

/// The same with extra environment variables.
pub fn git_with(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
    let output = git_output(dir, args, env)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// git's whole output, whatever its exit status.
pub fn git_output(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .envs(env.iter().copied())
        .output()
        .map_err(|e| format!("running git: {e}"))
}
