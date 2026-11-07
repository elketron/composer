use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::config_sections::SandboxConfig;
use crate::sandbox::sandbox_spec::{SYSTEM, base_args};
use crate::sandbox::{GitRepo, Mount, check_bwrap};
use crate::tools::expand_home;

/// The machine's `[sandbox]` settings for composer's own git ([`configure_git_jail`]); the
/// defaults (on) until a server sets them.
static CONFIG: RwLock<Option<SandboxConfig>> = RwLock::new(None);

/// Composer's own git follows `config` from now on (on or off, which system dirs and toolchains
/// a user's hooks see). One setting per process: the sandbox's switch is the machine's.
pub fn configure_git_jail(config: &SandboxConfig) {
    *CONFIG.write().expect("git jail config lock") = Some(config.clone());
}

fn config() -> SandboxConfig {
    CONFIG
        .read()
        .expect("git jail config lock")
        .clone()
        .unwrap_or_else(|| crate::Config::defaults().sandbox)
}

/// `git -C dir <args>` as composer runs it on a repository a sandboxed run may have written to:
/// in a `bwrap` jail where hooks, filters, `core.fsmonitor` and the like can only reach that
/// repository (its checkout and `.git`, or a worktree and its main `.git`; its other worktrees
/// read-only), the worktree paths
/// `git worktree add|remove` names, read-only system and toolchains, `~/.gitconfig`; an
/// empty `$HOME`, `/tmp` and `/run`; no network. Returns the program and its arguments. Off: plain
/// `git`. Refused when the jail can't be built or the repository can't be trusted.
pub fn git_jailed(dir: &Path, args: &[&str]) -> Result<(OsString, Vec<OsString>), String> {
    // Inside, the repository's other worktrees are there (read-only) but a gc must never prune
    // one it can't see.
    let mut git: Vec<OsString> = vec![host_git()];
    git.extend(["-c", "gc.worktreePruneExpire=never", "-C"].map(OsString::from));
    git.push(dir.into());
    git.extend(args.iter().map(OsString::from));
    let config = config();
    if !config.enabled {
        let program = git.remove(0);
        return Ok((program, git));
    }
    let bwrap = Path::new("bwrap");
    check_bwrap(bwrap)?;
    let home = expand_home("~");
    let mut jail = base_args(&home);
    jail.push("--unshare-net".into());
    for mount in mounts(dir, args, &config)? {
        jail.extend(mount.args());
    }
    jail.extend(["--setenv".into(), "HOME".into(), home.into_os_string()]);
    jail.extend(["--setenv", "TMPDIR", "/tmp", "--new-session"].map(OsString::from));
    jail.extend(["--chdir".into(), dir.into(), "--".into()]);
    jail.extend(git);
    Ok((bwrap.into(), jail))
}

fn mounts(dir: &Path, args: &[&str], config: &SandboxConfig) -> Result<Vec<Mount>, String> {
    let ro = SYSTEM
        .iter()
        .map(PathBuf::from)
        .chain(config.system_dirs.iter().map(|p| expand_home(p)))
        .chain(config.toolchains.iter().map(|p| expand_home(p)))
        .chain(["~/.gitconfig", "~/.config/git"].map(expand_home));
    let mut mounts: Vec<Mount> = ro.map(Mount::Ro).collect();
    let writable = match GitRepo::discover(dir)? {
        Some(repo) => {
            mounts.extend(repo.worktrees().into_iter().map(Mount::Ro));
            repo.writable()
        }
        // Not in a repository yet (`git init`).
        None => vec![dir.to_path_buf()],
    };
    mounts.extend(writable.into_iter().map(Mount::Rw));
    mounts.extend(worktree_paths(args)?.into_iter().map(Mount::Rw));
    Ok(mounts)
}

/// The folders holding the worktrees `git worktree add|remove` names (composer's own: a ticket's
/// worktree in `state/worktrees/`, a scratch merge worktree), made if missing. The parent, not
/// the worktree: a mount point can't be removed.
fn worktree_paths(args: &[&str]) -> Result<Vec<PathBuf>, String> {
    let ["worktree", "add" | "remove", rest @ ..] = args else {
        return Ok(Vec::new());
    };
    let mut parents = Vec::new();
    for parent in rest
        .iter()
        .map(Path::new)
        .filter(|p| p.is_absolute())
        .filter_map(Path::parent)
    {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        parents.push(parent.to_path_buf());
    }
    Ok(parents)
}

/// The git `PATH` finds here, symlinks resolved (inside, `PATH` may name folders that aren't
/// there; `/usr/bin` and the toolchains are); `git` when there is none.
fn host_git() -> OsString {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join("git"))
        .find(|git| git.is_file())
        .and_then(|git| std::fs::canonicalize(git).ok())
        .map_or_else(|| "git".into(), PathBuf::into_os_string)
}
