use std::path::{Path, PathBuf};

use crate::harness::ClaudeToken;
use crate::home::Home;
use crate::sandbox::{Mount, SandboxSpec};
use crate::server::Server;

/// What the user's own `~/.claude` lends every workspace's runs, read-only: their instructions,
/// settings, skills, agents, commands and plugins. Claude Code's memory, history and sessions stay
/// in the workspace's own `.claude`.
const SHARED: [&str; 6] = [
    "CLAUDE.md",
    "settings.json",
    "skills",
    "agents",
    "commands",
    "plugins",
];

/// The one file of the user's own `~/.claude` a run gets read-write: its login. A sandboxed run
/// without a stored token authenticates as the user, and an OAuth refresh writes through to the
/// real file (decided 2026-11-15): every workspace's runs share one live login, so a revoke or a
/// rate limit hits them all. The shared files above stay read-only, and memory, history and
/// sessions stay per workspace.
const SHARED_RW: &str = ".credentials.json";

/// The sandbox of a Claude Code run in `folder` (a project or ticket worktree), and its login:
/// the stored token when the home has one, else the user's own login file, mounted read-write.
/// Refused when neither exists. `None` when the sandbox is off: the run then goes as before, on
/// the machine's own login.
pub fn claude_code_world(
    server: &Server,
    folder: &Path,
    attachments: &[&Path],
    read_dirs: &[PathBuf],
) -> Result<Option<(SandboxSpec, Option<ClaudeToken>)>, String> {
    let spec = server.sandbox();
    if !spec.enabled() {
        return Ok(None);
    }
    let token = ClaudeToken::load(&Home::new(server.home.home_root()));
    let user_home = spec.user_home().to_path_buf();
    // Fails closed: without a stored token the run logs in through the mounted login file; with
    // neither, it does not start.
    if token.is_none() && !login_file(&user_home).exists() {
        return Err(ClaudeToken::missing());
    }
    // For a worktree, also the main repository's `.git` (commits need it), as for a recipe.
    let mut spec = spec.with_project(folder)?.in_dir(folder);
    for dir in read_dirs {
        spec = spec.with_ro(dir);
    }
    for dir in attachments {
        spec = spec.with_mount(Mount::Ro(dir.to_path_buf()));
    }
    spec = spec.with_mount(Mount::Ro(server.worker_exe.clone()));
    for mount in claude_home(&server.home.state().join("claude"), &user_home)? {
        spec = spec.with_mount(mount);
    }
    Ok(Some((spec, token)))
}

/// The workspace's own `.claude` and `.claude.json` at `~`, then the user's shared files over it:
/// read-only, except their login ([`SHARED_RW`]), the one file mounted read-write.
fn claude_home(own: &Path, user_home: &Path) -> Result<Vec<Mount>, String> {
    let dot = own.join(".claude");
    let json = own.join(".claude.json");
    std::fs::create_dir_all(&dot).map_err(|e| format!("{}: {e}", dot.display()))?;
    if !json.exists() {
        std::fs::write(&json, "{}\n").map_err(|e| format!("{}: {e}", json.display()))?;
    }
    let mut mounts = vec![
        Mount::RwAt {
            outside: dot,
            inside: user_home.join(".claude"),
        },
        Mount::RwAt {
            outside: json,
            inside: user_home.join(".claude.json"),
        },
    ];
    let shared = SHARED
        .iter()
        .map(|name| user_home.join(".claude").join(name))
        .filter(|path| path.exists())
        .map(Mount::Ro);
    mounts.extend(shared);
    let login = login_file(user_home);
    if login.exists() {
        mounts.push(Mount::Rw(login));
    }
    Ok(mounts)
}

/// The user's own login file: mounted into a run that has no stored token.
fn login_file(user_home: &Path) -> PathBuf {
    user_home.join(".claude").join(SHARED_RW)
}
