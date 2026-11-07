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

/// The sandbox of a Claude Code run in `folder` (a project or ticket worktree), and its login.
/// `None` when the sandbox is off: the run then goes as before, on the machine's own login.
pub fn claude_code_world(
    server: &Server,
    folder: &Path,
    attachments: &[&Path],
    read_dirs: &[PathBuf],
) -> Result<Option<(SandboxSpec, ClaudeToken)>, String> {
    let spec = server.sandbox();
    if !spec.enabled() {
        return Ok(None);
    }
    let token =
        ClaudeToken::load(&Home::new(server.home.home_root())).ok_or_else(ClaudeToken::missing)?;
    let user_home = spec.user_home().to_path_buf();
    let mut spec = spec.with_rw(folder).in_dir(folder);
    if let Some(common) = git_common_dir(folder) {
        spec = spec.with_rw(common);
    }
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

/// The workspace's own `.claude` and `.claude.json` at `~`, then the user's shared files over it.
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
    Ok(mounts)
}

/// For a git worktree, the main repository's `.git` (its `.git` file points into it): commits
/// need it writable, while the main checkout's files stay out.
fn git_common_dir(folder: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(folder.join(".git")).ok()?;
    let gitdir = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
    let gitdir = if gitdir.is_absolute() {
        gitdir
    } else {
        folder.join(gitdir)
    };
    // `<repo>/.git/worktrees/<name>` → `<repo>/.git`.
    let common = gitdir.parent()?.parent()?;
    (common.file_name()? == ".git" || common.join("HEAD").is_file()).then(|| common.to_path_buf())
}
