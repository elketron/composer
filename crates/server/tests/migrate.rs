//! `composer migrate` (ticket 0145): a home from before workspaces becomes the `default`
//! workspace with its pages, sessions, token, tickets and worktrees intact.

mod support;

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use composer_server::{
    Config, Home, Server, WorkspaceHome, WorkspaceName, init_workspace, migrate_home,
};
use support::MockProvider;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// An old single-workspace home at `root`: what `init` made before workspaces, plus a page, a
/// session, a config with per-project settings, and a ticket worktree of a git project.
async fn old_home(root: &Path, project: &Path) -> String {
    let old = WorkspaceHome::new(root);
    init_workspace(&old).await.unwrap();
    std::fs::write(
        old.config_file(),
        "# Nothing listens on port 1: migrate must not think a server is running.\n\
         [server]\nbind = \"127.0.0.1:1\"\n\n\
         [agent]\nslots = 3\n\n\
         # The project runs on Claude Code.\n\
         [projects.\"~/Projects/x\"]\nexternal_harnesses = [\"claude-code\"]\n\n\
         [mcp.wayai]\nurl = \"http://127.0.0.1:7878/mcp\"\n",
    )
    .unwrap();
    std::fs::write(
        old.sites().join("memory/zebra.md"),
        "---\ntitle: Zebra\n---\n# Zebra\n",
    )
    .unwrap();
    let config = old.load_config().unwrap();
    let server = Server::new(old.clone(), config, Arc::new(MockProvider::new(vec![]))).unwrap();
    let session = server
        .sessions
        .create("default", None)
        .unwrap()
        .id()
        .to_string();
    drop(server);

    std::fs::create_dir_all(project).unwrap();
    git(project, &["init", "-q", "-b", "main"]);
    std::fs::write(project.join("README.md"), "# x\n").unwrap();
    git(project, &["add", "."]);
    git(
        project,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "init",
        ],
    );
    let worktree = old.state().join("worktrees/0001");
    git(
        project,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "ticket/0001",
            worktree.to_str().unwrap(),
        ],
    );
    session
}

#[tokio::test]
async fn migrates_into_the_default_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("home");
    let project = dir.path().join("project");
    let session = old_home(&root, &project).await;
    let old_token = std::fs::read_to_string(root.join("state/token")).unwrap();

    let home = Home::new(&root);
    assert!(home.is_single_workspace());
    let report = migrate_home(&home).await.unwrap();
    assert!(
        report.backup.is_file(),
        "backup written: {}",
        report.backup.display()
    );
    assert!(report.backup.starts_with(dir.path()), "next to the home");
    assert!(!home.is_single_workspace());

    let ws = home.workspace(&WorkspaceName::default_workspace());
    // The web, the state and the token moved; the old root files are gone.
    assert!(ws.sites().join("memory/zebra.md").is_file());
    assert_eq!(std::fs::read_to_string(ws.token_file()).unwrap(), old_token);
    assert!(!root.join("config.toml").exists());
    assert!(!root.join("state/token").exists());
    // The library and templates are the home's; template bases stayed with them.
    assert!(home.library().join("home/index.md").is_file());
    assert!(
        !home.library().join("memory/zebra.md").exists(),
        "the library is stock only"
    );
    assert!(home.templates().join("app").is_dir());
    assert!(root.join("state/seed/templates").is_dir());
    assert!(ws.seed_base().join("sites").is_dir());
    assert!(!ws.seed_base().join("templates").exists());

    // The config split by table, comments kept, and layering gives every setting back.
    let global = std::fs::read_to_string(home.global_config_file()).unwrap();
    let own = std::fs::read_to_string(ws.config_file()).unwrap();
    assert!(global.contains("[agent]") && !global.contains("projects"));
    assert!(own.contains("# The project runs on Claude Code.\n[projects."));
    assert!(own.contains("[mcp.wayai]"));
    let config: Config = ws.load_config().unwrap();
    assert_eq!(config.agent.slots, 3);
    assert!(config.projects.settings.contains_key("~/Projects/x"));
    assert!(config.mcp.contains_key("wayai"));

    // The workspace opens with its session, and finds the page.
    let server = Server::new(ws.clone(), config, Arc::new(MockProvider::new(vec![]))).unwrap();
    assert!(server.sessions.list().iter().any(|s| s.id == session));
    let hits = server
        .index
        .read()
        .unwrap()
        .search(&composer_core::Query::parse("zebra"));
    assert_eq!(hits.len(), 1);

    // The ticket worktree moved and its repository knows the new place.
    let moved = ws.state().join("worktrees/0001");
    assert_eq!(report.worktrees, vec![(moved.clone(), true)]);
    let list = git(&project, &["worktree", "list", "--porcelain"]);
    assert!(list.contains(moved.to_str().unwrap()), "{list}");
    assert_eq!(
        git(&moved, &["branch", "--show-current"]).trim(),
        "ticket/0001"
    );

    // Once migrated, there is nothing left to migrate.
    assert!(migrate_home(&home).await.is_err());
}

#[tokio::test]
async fn refuses_while_a_server_answers_on_the_old_address() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("home");
    let old = WorkspaceHome::new(&root);
    init_workspace(&old).await.unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::fs::write(old.config_file(), format!("[server]\nbind = \"{addr}\"\n")).unwrap();

    let error = migrate_home(&Home::new(&root))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("stop it"), "{error}");
    assert!(root.join("config.toml").is_file(), "nothing moved");
    assert!(!root.join("workspaces").exists());
}
