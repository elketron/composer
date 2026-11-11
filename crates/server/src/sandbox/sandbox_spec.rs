use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::config_sections::SandboxConfig;
use crate::sandbox::{Mount, check_bwrap};
use crate::tools::expand_home;
use crate::workspace_home::WorkspaceHome;

/// The world of one spawn of one workspace, turned into a `bwrap` command by [`Self::command`].
///
/// Read-only: `/usr`, `/etc`, the configured system dirs, toolchains, `~/.gitconfig`, the home's
/// `global/` and `templates/`, `binds_ro`. Read-write: the workspace's `sites/`, the spawn's
/// project (or ticket worktree, with the main repo's `.git`), the shared caches, `binds_rw`.
/// Empty: `$HOME`, `/run`, `/tmp`. Projects of other workspaces inside a bind are hidden; a bind
/// into the composer home (other than this workspace's sites and worktrees), around it, or into
/// another workspace's project is refused.
#[derive(Debug, Clone)]
pub struct SandboxSpec {
    config: SandboxConfig,
    bwrap: PathBuf,
    /// The user's `$HOME`, empty inside.
    user_home: PathBuf,
    /// The composer home (`~/.composer`).
    composer_home: PathBuf,
    /// What no bind may reach: composer's state (tokens), the library, other workspaces, this
    /// workspace's config.
    protected: Vec<PathBuf>,
    sites: PathBuf,
    worktrees: PathBuf,
    /// Where `.mcpb` bundles are unpacked (`state/mcps`): a server may work there.
    mcps: PathBuf,
    workspace_root: PathBuf,
    /// Other workspaces' projects: hidden wherever a bind would show them.
    foreign: Vec<PathBuf>,
    ro: Vec<PathBuf>,
    rw: Vec<PathBuf>,
    /// Composer's own mounts (a run's `.claude`, its attachments): not checked like binds.
    own: Vec<Mount>,
    chdir: Option<PathBuf>,
    tty: bool,
}

/// What every spawn sees read-only, wherever it comes from.
pub(crate) const SYSTEM: [&str; 2] = ["/usr", "/etc"];

impl SandboxSpec {
    /// The base world of a spawn of `workspace`; `foreign` are the projects other workspaces own.
    pub fn new(workspace: &WorkspaceHome, config: SandboxConfig, foreign: Vec<PathBuf>) -> Self {
        let user_home = expand_home("~");
        Self {
            protected: protected(workspace),
            config,
            bwrap: PathBuf::from("bwrap"),
            user_home,
            composer_home: canonical(workspace.home_root()),
            sites: canonical(&workspace.sites()),
            worktrees: canonical(&workspace.worktrees()),
            mcps: canonical(workspace.home_root()).join("state").join("mcps"),
            workspace_root: workspace.root().to_path_buf(),
            foreign: foreign.iter().map(|p| canonical(p)).collect(),
            ro: Vec::new(),
            rw: Vec::new(),
            own: Vec::new(),
            chdir: None,
            tty: false,
        }
    }

    /// A project (or ticket worktree) read-write.
    pub fn with_rw(mut self, path: impl Into<PathBuf>) -> Self {
        self.rw.push(path.into());
        self
    }

    /// A folder or file read-only.
    pub fn with_ro(mut self, path: impl Into<PathBuf>) -> Self {
        self.ro.push(path.into());
        self
    }

    /// A mount composer itself chooses (never a path from config or an agent): not checked
    /// against the protected parts of the home, so it may show a piece of this workspace's state.
    pub(crate) fn with_mount(mut self, mount: Mount) -> Self {
        self.own.push(mount);
        self
    }

    /// The user's `$HOME` (empty inside, apart from what is mounted into it).
    pub fn user_home(&self) -> &Path {
        &self.user_home
    }

    /// The workspace's sites (always mounted read-write).
    pub fn sites(&self) -> &Path {
        &self.sites
    }

    /// Where the program starts.
    pub fn in_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.chdir = Some(dir.into());
        self
    }

    /// A terminal shell: keeps the controlling terminal (job control) instead of a new session.
    pub fn tty(mut self) -> Self {
        self.tty = true;
        self
    }

    /// Another `bwrap` (tests, unusual installs).
    pub fn bwrap_at(mut self, program: impl Into<PathBuf>) -> Self {
        self.bwrap = program.into();
        self
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled
    }

    /// The mounts, in order; refused when a bind reaches where it may not.
    pub fn mounts(&self) -> Result<Vec<Mount>, String> {
        let expand =
            |paths: &[String]| -> Vec<PathBuf> { paths.iter().map(|p| expand_home(p)).collect() };
        let mut mounts: Vec<Mount> = SYSTEM.iter().map(|p| Mount::Ro(p.into())).collect();
        mounts.extend(expand(&self.config.system_dirs).into_iter().map(Mount::Ro));
        mounts.extend(expand(&self.config.toolchains).into_iter().map(Mount::Ro));
        for file in ["~/.gitconfig", "~/.config/git"] {
            mounts.push(Mount::Ro(expand_home(file)));
        }
        mounts.extend(expand(&self.config.caches).into_iter().map(Mount::Rw));
        let templates = self.composer_home.join("templates");
        mounts.push(Mount::Ro(self.composer_home.join("global")));
        mounts.push(Mount::Ro(templates.clone()));
        // Site UIs made before workspaces look for the sdk at `<workspace>/templates`.
        mounts.push(Mount::RoAt {
            outside: templates,
            inside: self.workspace_root.join("templates"),
        });
        mounts.push(Mount::Rw(self.sites.clone()));
        let ro = expand(&self.config.binds_ro)
            .into_iter()
            .chain(self.ro.clone());
        for path in ro {
            mounts.push(Mount::Ro(self.allowed(&path)?));
        }
        let rw = expand(&self.config.binds_rw)
            .into_iter()
            .chain(self.rw.clone());
        for path in rw {
            mounts.push(Mount::Rw(self.allowed(&path)?));
        }
        mounts.extend(expand(&self.config.devices).into_iter().map(Mount::Dev));
        mounts.extend(self.own.iter().cloned());
        let hidden = self.hidden(&mounts);
        mounts.extend(hidden.into_iter().map(Mount::Hide));
        Ok(mounts)
    }

    /// `program` to run in this world. Not enabled: as it is. Enabled and `bwrap` can't build a
    /// namespace, or a bind isn't allowed: refused.
    #[allow(
        clippy::disallowed_methods,
        reason = "the sandbox's own bwrap (or the program as it is when the user turned it off)"
    )]
    pub fn command(&self, program: impl AsRef<OsStr>) -> Result<Command, String> {
        let (program, args) = self.wrap(program)?;
        let mut command = Command::new(program);
        command.args(args);
        if let (false, Some(dir)) = (self.enabled(), &self.chdir) {
            command.current_dir(dir);
        }
        Ok(command)
    }

    /// `program` in this world as the program to start and its first arguments
    /// (`bwrap`, `[... -- program]`), for a caller that builds the command itself (inside a
    /// systemd scope). Not enabled: `program` as it is; the caller sets its directory.
    pub fn wrap(&self, program: impl AsRef<OsStr>) -> Result<(OsString, Vec<OsString>), String> {
        let program = program.as_ref().to_os_string();
        if !self.enabled() {
            return Ok((program, Vec::new()));
        }
        check_bwrap(&self.bwrap)?;
        let mut args = self.args()?;
        args.extend(["--".into(), program]);
        Ok((self.bwrap.clone().into_os_string(), args))
    }

    /// Everything before `-- program`.
    pub fn args(&self) -> Result<Vec<OsString>, String> {
        let home = self.user_home.as_os_str();
        let mut args = base_args(&self.user_home);
        for mount in self.mounts()? {
            args.extend(mount.args());
        }
        args.extend(["--setenv".into(), "HOME".into(), home.into()]);
        args.extend(["--setenv", "TMPDIR", "/tmp"].map(OsString::from));
        if !self.tty {
            args.push("--new-session".into());
        }
        if let Some(dir) = &self.chdir {
            args.extend(["--chdir".into(), dir.into()]);
        }
        Ok(args)
    }

    /// `path` when it may be bound: not into or around composer's state, the library, another
    /// workspace or this one's config (its sites and worktrees, and the unpacked MCP bundles, are fine), not into another
    /// workspace's project.
    fn allowed(&self, path: &Path) -> Result<PathBuf, String> {
        let real = canonical(path);
        let own = real.starts_with(&self.sites)
            || real.starts_with(&self.worktrees)
            || real.starts_with(&self.mcps);
        let refuse = |why: &str| Err(format!("the sandbox can't show {}: {why}", path.display()));
        if !own && self.protected.iter().any(|p| real.starts_with(p)) {
            return refuse(
                "it is part of the composer home (state and tokens, the library, other \
                 workspaces)",
            );
        }
        if self.protected.iter().any(|p| p.starts_with(&real)) {
            return refuse(
                "it holds part of the composer home (state and tokens, the library, other \
                 workspaces)",
            );
        }
        if let Some(owned) = self.foreign.iter().find(|f| real.starts_with(f)) {
            return refuse(&format!(
                "it is inside {}, a project of another workspace",
                owned.display()
            ));
        }
        Ok(path.to_path_buf())
    }

    /// Other workspaces' projects some mount would show.
    fn hidden(&self, mounts: &[Mount]) -> Vec<PathBuf> {
        let shown = |project: &Path| {
            mounts
                .iter()
                .filter_map(Mount::inside)
                .any(|inside| project.starts_with(canonical(inside)))
        };
        self.foreign
            .iter()
            .filter(|project| shown(project))
            .cloned()
            .collect()
    }
}

/// What every sandbox starts from: its own pid, ipc, uts and cgroup namespaces and hostname, gone
/// with composer; fresh `/proc` and `/dev`; empty `/tmp`, `/run` and `$HOME` (`home`); the
/// host's `/bin`, `/lib`... links (or folders).
pub(crate) fn base_args(home: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--die-with-parent",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-uts",
        "--unshare-cgroup-try",
        "--hostname",
        "sandbox",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--tmpfs",
        "/run",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.extend(["--tmpfs".into(), home.into()]);
    // As on the host: merged-/usr links (Arch: all to usr/bin, usr/lib; Ubuntu: lib64 to
    // usr/lib64), or folders of their own.
    for top in ["/bin", "/sbin", "/lib", "/lib32", "/lib64"] {
        match std::fs::read_link(top) {
            Ok(target) => args.extend(["--symlink".into(), target.into(), top.into()]),
            Err(_) => args.extend(Mount::Ro(top.into()).args()),
        }
    }
    args
}

/// `path` with symlinks resolved, or as it is when it doesn't exist.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The parts of `workspace`'s home no bind may reach: state and config, the installed MCP bundles
/// (`mcps/`: what is there runs as an MCP server), the library and the other workspaces.
fn protected(workspace: &WorkspaceHome) -> Vec<PathBuf> {
    let root = canonical(workspace.root());
    let home = canonical(workspace.home_root());
    let mut parts = vec![
        root.join("state"),
        root.join("config.toml"),
        home.join("mcps"),
    ];
    if workspace.is_standalone() {
        return parts;
    }
    parts.extend([home.join("state"), home.join("sites")]);
    let others = std::fs::read_dir(home.join("workspaces"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| canonical(&e.path()))
        .filter(|other| *other != root);
    parts.extend(others);
    parts
}
