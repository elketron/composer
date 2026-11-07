//! The sandbox (plan phase 3, isolation layer 3): every spawn an agent or a site controls runs in a
//! `bwrap` namespace built from its workspace. Its sites and project are read-write; the system,
//! toolchains, `global/` and the home's `templates/` are read-only; `$HOME`, `/run` and `/tmp` are
//! empty and private. Other workspaces, the library and every `state/` (tokens) are not inside.
//! The network is shared, so the composer server stays reachable.
//!
//! It fails closed: when it is on and `bwrap` can't build a namespace, the spawn is refused.

mod bwrap;
mod mount;
mod sandbox_spec;

pub use bwrap::check_bwrap;
pub use mount::Mount;
pub use sandbox_spec::SandboxSpec;
