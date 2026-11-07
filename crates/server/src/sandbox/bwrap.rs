use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};

/// Whether `bwrap` at `program` can build a namespace here (a tiny one, once per program and
/// process): `Err` says what to do about it.
pub fn check_bwrap(program: &Path) -> Result<(), String> {
    static CHECKED: OnceLock<Mutex<HashMap<PathBuf, Result<(), String>>>> = OnceLock::new();
    let checked = CHECKED.get_or_init(Mutex::default);
    if let Some(known) = checked.lock().expect("bwrap lock").get(program) {
        return known.clone();
    }
    let result = probe(program);
    match &result {
        Ok(()) => tracing::info!("sandbox: {} works", program.display()),
        Err(e) => tracing::warn!("sandbox: {e}"),
    }
    checked
        .lock()
        .expect("bwrap lock")
        .insert(program.to_path_buf(), result.clone());
    result
}

#[allow(clippy::disallowed_methods, reason = "probes bwrap itself")]
fn probe(program: &Path) -> Result<(), String> {
    let output = std::process::Command::new(program)
        .args([
            "--unshare-pid",
            "--ro-bind",
            "/",
            "/",
            "--tmpfs",
            "/tmp",
            // Whatever PATH composer has, `true` is found.
            "--setenv",
            "PATH",
            "/usr/bin:/bin",
            "--",
            "true",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    let refused = |why: String| {
        format!(
            "the sandbox can't start ({why}). Spawns that run in it (recipes, Claude Code, MCP \
             servers) are refused until it can: install bubblewrap (`bwrap`) and allow \
             unprivileged user namespaces, or set `[sandbox] enabled = false` in \
             global/config.toml to run them unsandboxed"
        )
    };
    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(refused(format!(
            "`{}` failed: {}",
            program.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))),
        Err(e) => Err(refused(format!("`{}`: {e}", program.display()))),
    }
}
