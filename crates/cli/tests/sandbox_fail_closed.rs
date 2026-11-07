//! Fail closed (ticket 0160): with the sandbox on and a `bwrap` that can't build a namespace
//! (first on `PATH`), every kind of spawn is refused, and the probe never runs, unsandboxed or
//! otherwise. (Without a login token a Claude Code run is refused too: `claude_code_sandbox.rs`.)
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod sandbox_world;
mod support;

use sandbox_world::world;

#[tokio::test]
async fn a_broken_bwrap_refuses_every_spawn() {
    let fake = tempfile::tempdir().unwrap();
    let bwrap = fake.path().join("bwrap");
    std::fs::write(
        &bwrap,
        "#!/bin/sh\necho 'bwrap: No permissions to create new namespace' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&bwrap, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        fake.path().display(),
        std::env::var("PATH").unwrap()
    );
    // SAFETY: set before any server or thread reads it; the only test in this binary.
    unsafe { std::env::set_var("PATH", path) };
    let w = world().await;

    let refused = w.recipe().await.unwrap_err();
    assert!(refused.contains("namespace"), "{refused}");
    let (state, error) = w.mcp().await;
    assert_eq!(state, "failed");
    assert!(error.unwrap_or_default().contains("namespace"));
    let refused = w.claude().await.unwrap_err();
    assert!(refused.contains("namespace"), "{refused}");
    for kind in ["recipe", "mcp", "claude"] {
        assert!(w.report(kind).is_none(), "{kind} ran");
    }
}
