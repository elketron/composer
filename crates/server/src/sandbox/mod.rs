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
//! - recipes (`actions/run.rs`): a site's `just` recipe, with the project it runs on;
//! - Claude Code runs (`harness/claude_code_world.rs`): the ticket's worktree and the main repo's
//!   `.git`, the workspace's own `~/.claude`, the login token in its env;
//! - stdio MCP servers (`mcp_clients/mcp_client.rs`): the workspace's sites plus the entry's
//!   `binds_ro`/`binds_rw`. (MCP over HTTP is a connection, not a spawn: untouched.)
//!
//! Outside, each with an `#[allow(clippy::disallowed_methods, reason = ..)]` (clippy refuses any
//! other `Command::new`):
//! - composer's own work on paths it chose: `git` (diffs, merges, worktrees, `merge-file` and
//!   `--no-index` diffs of scratch files), `difft`, `pdftotext`, `tar` (migration backups);
//! - the `composer worker` process (own-harness workers: composer's code; what they spawn goes
//!   through here) and `systemd-run`/`systemctl` around runs;
//! - `bwrap` itself, and `just --dump`/`--show` (reads a justfile, runs none of it);
//! - what the user starts: launcher entries, the TUI's editor and opener, `tailscale`;
//! - `claude auth status`, only when the sandbox is off.
//!
//! Known gap: composer's own `git` runs outside on repos whose `.git` a sandboxed run can write
//! (a Claude Code run commits there), and repo config (`core.fsmonitor`, hooks, filters) can name
//! commands git then runs. Hardening composer's git calls is a follow-up (ticket 0160).

mod bwrap;
mod mount;
mod sandbox_spec;

pub use bwrap::check_bwrap;
pub use mount::Mount;
pub use sandbox_spec::SandboxSpec;
