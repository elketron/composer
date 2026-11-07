//! The sandbox review's boundary suite (ticket 0160): the same probe from inside each kind of
//! spawn (a recipe, a stdio MCP server, a Claude Code run) of workspace `default`, in a home with
//! a second workspace. None of them sees or writes the other workspace, the library, any `state/`
//! (tokens, the Claude Code login), the user's `$HOME` (`~/.ssh`, the real Claude credentials),
//! another project or the host's `/tmp`; each has its own pid, ipc, uts and cgroup namespaces;
//! each writes its own sites and `/tmp`. A Claude Code run works in its worktree, not the main
//! checkout, with its token in its env.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod sandbox_world;
mod support;

use sandbox_world::{failed, world};

#[tokio::test]
async fn every_kind_of_spawn_stays_in_its_workspace() {
    // A terminal's shell (0176): a plain one, so no shell setup wizard eats the command.
    // SAFETY: before the server starts; the only test in this binary.
    unsafe { std::env::set_var("SHELL", "/bin/sh") };
    let w = world().await;

    let (code, output) = w.recipe().await.unwrap();
    assert_eq!(code, Some(0), "{output}");
    let (state, error) = w.mcp().await;
    assert_eq!(state, "ready", "{error:?}");
    let ticket = w.claude().await.unwrap();
    assert!(ticket.contains("status: done"), "{ticket}\n{}", w.logs());
    w.terminal().await.unwrap();

    for kind in ["recipe", "mcp", "claude", "terminal"] {
        let report = w
            .report(kind)
            .unwrap_or_else(|| panic!("{kind} didn't report"));
        assert_eq!(failed(&report), Vec::<String>::new(), "{kind}: {report:#}");
        for check in [
            "hidden: claude-token",
            "hidden: b's page",
            "hidden: ~/.claude/.credentials.json",
            "own pid namespace",
            "a's state: only the run's own parts",
        ] {
            assert!(report.get(check).is_some(), "{kind}: no `{check}`");
        }
        assert!(
            w.a.sites().join(format!("from-{kind}")).exists(),
            "{kind}: its own sites"
        );
        assert_eq!(
            w.leaked_writes(kind),
            Vec::<std::path::PathBuf>::new(),
            "{kind}"
        );
    }
}
