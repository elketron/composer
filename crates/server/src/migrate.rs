//! `composer migrate`: moves a home from before workspaces (`config.toml`, `sites/`, `state/` at
//! the root) into the workspace layout, as its `default` workspace. One way; a backup tarball is
//! written next to the home first.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::error::{Result, ServerError};
use crate::home::Home;
use crate::init::init_home;
use crate::workspace_home::WorkspaceHome;
use crate::workspace_name::WorkspaceName;

/// Top-level config tables that belong to a workspace; everything else becomes global.
const WORKSPACE_TABLES: [&str; 3] = ["projects", "mcp", "launcher"];

/// Under `state/`, what stays with the home rather than moving into the workspace: the bases
/// and review copies of the shared templates.
const SHARED_STATE: [&str; 2] = ["seed/templates", "updates/templates"];

/// What `migrate` did.
#[derive(Debug, Default)]
pub struct MigrateReport {
    pub backup: PathBuf,
    pub workspace: PathBuf,
    /// Git worktrees moved with the state, and whether `git worktree repair` fixed their links.
    pub worktrees: Vec<(PathBuf, bool)>,
    pub warnings: Vec<String>,
}

/// Migrates `home` into the workspace layout. Refuses when the home isn't a single-workspace
/// one, or when a server still answers on its configured address.
pub async fn migrate_home(home: &Home) -> Result<MigrateReport> {
    if !home.is_single_workspace() {
        return Err(ServerError::Invalid(format!(
            "{} is not a home from before workspaces (it needs config.toml at the root and no \
             workspaces/ folder); nothing to migrate",
            home.root().display()
        )));
    }
    let old_config = home.root().join("config.toml");
    let text = std::fs::read_to_string(&old_config).map_err(|e| ServerError::io(&old_config, e))?;
    refuse_if_serving(&text)?;

    let mut report = MigrateReport {
        backup: backup(home)?,
        ..MigrateReport::default()
    };
    let workspace = home.workspace(&WorkspaceName::default_workspace());
    report.workspace = workspace.root().to_path_buf();
    create_dir(&workspace.state())?;

    rename(&home.library(), &workspace.sites())?;
    move_state(home, &workspace)?;
    let (global, own) = split_config(&text);
    create_dir(&home.global())?;
    write(&home.global_config_file(), &global)?;
    write(&workspace.config_file(), &own)?;
    std::fs::remove_file(&old_config).map_err(|e| ServerError::io(&old_config, e))?;

    // The library (and anything else a fresh home has) from the stock files; the workspace
    // exists now, so init leaves it alone.
    init_home(home).await?;
    report.worktrees = repair_worktrees(&workspace.worktrees());
    report.warnings.extend(
        report
            .worktrees
            .iter()
            .filter(|(_, repaired)| !repaired)
            .map(|(path, _)| format!("`git worktree repair` failed in {}", path.display())),
    );
    Ok(report)
}

/// A server on the old home's address would keep writing to the folders being moved.
fn refuse_if_serving(config: &str) -> Result<()> {
    let bind = crate::config::Config::parse(config)?.server.bind;
    let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(bind.as_str()) else {
        return Ok(());
    };
    for addr in addrs {
        if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
            return Err(ServerError::Invalid(format!(
                "something answers on {bind} (a running composer server?): stop it, then run \
                 `composer migrate` again"
            )));
        }
    }
    Ok(())
}

/// `<home>.backup-<unix time>.tar.gz` next to the home, without ticket worktrees (they can be
/// large; their commits live in the repositories, and the folders are moved, not deleted).
#[allow(
    clippy::disallowed_methods,
    reason = "tar of the home before a migration, composer's own paths"
)]
fn backup(home: &Home) -> Result<PathBuf> {
    let root = home.root();
    let (Some(parent), Some(name)) = (root.parent(), root.file_name()) else {
        return Err(ServerError::Invalid(format!(
            "can't back up {}: it has no parent folder",
            root.display()
        )));
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let target = parent.join(format!("{}.backup-{stamp}.tar.gz", name.to_string_lossy()));
    let status = Command::new("tar")
        .arg("-czf")
        .arg(&target)
        .arg("--exclude")
        .arg(format!("{}/state/worktrees", name.to_string_lossy()))
        .arg("-C")
        .arg(parent)
        .arg(name)
        .status()
        .map_err(|e| ServerError::Invalid(format!("running tar for the backup: {e}")))?;
    if !status.success() {
        return Err(ServerError::Invalid(format!(
            "the backup to {} failed (tar exited with {status}); nothing was moved",
            target.display()
        )));
    }
    Ok(target)
}

/// Moves everything under `state/` into the workspace, except the shared templates' bases.
fn move_state(home: &Home, workspace: &WorkspaceHome) -> Result<()> {
    let state = home.state();
    let entries = match std::fs::read_dir(&state) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(ServerError::io(&state, e)),
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let target = workspace.state().join(&name);
        let keeps_shared = SHARED_STATE
            .iter()
            .any(|shared| Path::new(shared).starts_with(&name));
        if keeps_shared {
            move_children_except(&entry.path(), &target, &state)?;
        } else {
            rename(&entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Moves the children of `dir` (e.g. `state/seed/`) to `target`, except the shared ones.
fn move_children_except(dir: &Path, target: &Path, state: &Path) -> Result<()> {
    create_dir(target)?;
    let entries = std::fs::read_dir(dir).map_err(|e| ServerError::io(dir, e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let shared = path
            .strip_prefix(state)
            .is_ok_and(|rel| SHARED_STATE.iter().any(|s| Path::new(s) == rel));
        if !shared {
            rename(&path, &target.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Splits an old `config.toml` into the global part and the workspace's part, line by line so
/// comments stay with the table they precede.
fn split_config(text: &str) -> (String, String) {
    let mut global = String::new();
    let mut own =
        String::from("# This workspace's settings, on top of the home's global/config.toml.\n\n");
    let mut pending = String::new();
    let mut to_workspace = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            pending.push_str(line);
            pending.push('\n');
            continue;
        }
        if trimmed.starts_with('[') {
            to_workspace = WORKSPACE_TABLES.contains(&table_name(trimmed));
        }
        let out = if to_workspace { &mut own } else { &mut global };
        out.push_str(&pending);
        pending.clear();
        out.push_str(line);
        out.push('\n');
    }
    let out = if to_workspace { &mut own } else { &mut global };
    out.push_str(&pending);
    (global, own)
}

/// The top-level table of a header line: `[projects."~/x"]` → `projects`, `[[launcher]]` →
/// `launcher`.
fn table_name(header: &str) -> &str {
    let inner = header.trim_start_matches('[');
    let end = inner.find(['.', ']']).unwrap_or(inner.len());
    inner[..end].trim().trim_matches('"')
}

/// `git worktree repair` in each moved worktree, so its repository points at the new place.
#[allow(
    clippy::disallowed_methods,
    reason = "git worktree repair during a migration: one-off, server stopped; it repairs the back-pointers the git jail checks"
)]
fn repair_worktrees(dir: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join(".git").is_file())
        .map(|path| {
            let repaired = Command::new("git")
                .arg("-C")
                .arg(&path)
                .args(["worktree", "repair"])
                .output()
                .is_ok_and(|out| out.status.success());
            (path, repaired)
        })
        .collect()
}

fn rename(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        create_dir(parent)?;
    }
    std::fs::rename(from, to).map_err(|e| {
        ServerError::Invalid(format!(
            "moving {} to {}: {e}",
            from.display(),
            to.display()
        ))
    })
}

fn create_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| ServerError::io(dir, e))
}

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).map_err(|e| ServerError::io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_config_by_table_and_keeps_comments() {
        let old = "\
[provider]
base_url = \"http://x\"

[agent]
slots = 2

# The 3D projects run on Claude Code.
[projects.\"~/Projects/a\"]
external_harnesses = [\"claude-code\"]

# wayai
[mcp.wayai]
url = \"http://127.0.0.1:7878/mcp\"

[[launcher]]
name = \"blender\"
command = \"blender\"
folder = \"app\"

[claude_code]
read_dirs = []
";
        let (global, own) = split_config(old);
        assert!(global.contains("[provider]") && global.contains("[agent]"));
        assert!(global.contains("[claude_code]"));
        assert!(!global.contains("projects") && !global.contains("wayai"));
        assert!(own.contains("# The 3D projects run on Claude Code.\n[projects.\"~/Projects/a\"]"));
        assert!(own.contains("# wayai\n[mcp.wayai]"));
        assert!(own.contains("[[launcher]]"));
        assert!(!own.contains("[provider]"));
        // Both halves still parse, and layering them gives back every setting.
        let dir = tempfile::tempdir().unwrap();
        let (g, w) = (dir.path().join("g.toml"), dir.path().join("w.toml"));
        std::fs::write(&g, &global).unwrap();
        std::fs::write(&w, &own).unwrap();
        let layered = crate::config::Config::load_layered(&g, &w).unwrap();
        assert_eq!(layered, crate::config::Config::parse(old).unwrap());
    }

    #[test]
    fn table_names() {
        assert_eq!(table_name("[projects.\"~/x\"]"), "projects");
        assert_eq!(table_name("[[launcher]]"), "launcher");
        assert_eq!(table_name("[ mcp.wayai ]"), "mcp");
        assert_eq!(table_name("[provider]"), "provider");
    }
}
