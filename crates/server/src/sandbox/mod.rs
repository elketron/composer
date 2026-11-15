//! The sandbox (plan phase 3, isolation layer 3): every spawn an agent or a site controls runs in a
//! `bwrap` namespace built from its workspace. Its sites and project are read-write; the system,
//! toolchains, `global/` and the home's `templates/` are read-only; `$HOME`, `/run` and `/tmp` are
//! empty and private. Other workspaces, the library and every `state/` (tokens) are not inside.
//! The network is shared, so the composer server stays reachable.
//!
//! It fails closed: when it is on and `bwrap` can't build a namespace, the spawn is refused.
//!
//! # Every spawn
//!
//! In the sandbox (anything an agent or a site controls):
//! - recipes (`actions/run.rs`): a site's `just` recipe, with the project it runs on — and when
//!   that project is a ticket worktree, the main repository's `.git` read-write (`git` works on
//!   its branch: status, log, diff, commit; it can move any ref of that one repository, never
//!   another's). Only a repository that names the worktree back; a forged `.git` file refuses
//!   the recipe with the `git worktree repair` hint (`SandboxSpec::with_project`, ticket 0203);
//! - Claude Code runs (`harness/claude_code_world.rs`): the ticket's worktree and the main repo's
//!   `.git` (the same `with_project`), the workspace's own `~/.claude` (the user's shared files
//!   read-only on top, their `.credentials.json` the one file read-write), and a stored login
//!   token in its env;
//! - stdio MCP servers (`mcp_clients/mcp_client.rs`): the workspace's sites plus the entry's
//!   `binds_ro`/`binds_rw`. (MCP over HTTP is a connection, not a spawn: untouched.)
//! - composer's own `git` on a repository (ticket branches and merges, worktrees, diffs, the
//!   library): in the git jail (`git_jail.rs`): that repository only, no network. A run can plant
//!   hooks or repo config (`core.fsmonitor`, filter and merge drivers); they run in there.
//!
//! Outside, each with an `#[allow(clippy::disallowed_methods, reason = ..)]` (clippy refuses any
//! other `Command::new`):
//! - composer's own work on files it wrote: `git merge-file` and `git diff --no-index` of scratch
//!   copies, `difft`, `pdftotext`, `tar` (migration backups), and `git worktree repair` during a
//!   migration (one-off, server stopped; it repairs the back-pointers the jail checks);
//! - the `composer worker` process (own-harness workers: composer's code; what they spawn goes
//!   through here) and `systemd-run`/`systemctl` around runs;
//! - `bwrap` itself, and `just --dump`/`--show` (reads a justfile, runs none of it);
//! - what the user starts: launcher entries, the TUI's editor and opener, `tailscale`;
//! - `claude auth status`, only when the sandbox is off.
//!
//! Not covered: a `commit.gpgsign` user's merges fail in the git jail (no `~/.gnupg` inside). The
//! Claude Code login is shared on purpose: a run with no stored token mounts the user's
//! `~/.claude/.credentials.json` read-write, so it authenticates as them and a refresh writes
//! through - every workspace's runs read and replace one live login (decided 2026-11-15).

mod bwrap;
mod git_jail;
mod git_repo;
mod mount;
mod sandbox_spec;

pub use bwrap::check_bwrap;
pub use git_jail::{configure_git_jail, git_jailed};
pub use git_repo::GitRepo;
pub use mount::Mount;
pub use sandbox_spec::SandboxSpec;
