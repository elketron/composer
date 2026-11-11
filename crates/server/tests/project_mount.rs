//! A work's `project/` link: the project root by default, the ticket's worktree while a stage
//! runs, and a write gate that follows the mode. The run-level swap is covered by the stage
//! tests; this pins the link and the read/write rule.

use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use composer_server::Config;
use composer_server::pipeline::{NewWork, new_work};
use composer_server::project::ProjectMode;
use support::start_with;

mod support;

#[tokio::test]
async fn a_worktree_link_swaps_and_read_refuses_writes() {
    let (dir, server, _provider, _addr) = start_with(Config::defaults(), vec![]).await;
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    let flow = dir.path().join("sites/flow");
    std::fs::create_dir_all(flow.join("stages")).unwrap();
    std::fs::write(
        flow.join("index.md"),
        "---\ntitle: Flow\ntype: pipeline\n---\n# Flow\n",
    )
    .unwrap();
    std::fs::write(
        flow.join("stages/01_impl.md"),
        "---\ntitle: Implement\ntype: stage\n---\n# 01 · Implement\n",
    )
    .unwrap();

    let work = new_work(
        &server,
        NewWork {
            pipeline: "flow",
            name: "save",
            title: None,
            project: Some(project.to_str().unwrap()),
            brief: Some("A save system."),
        },
        ChangedBy::Agent,
    )
    .unwrap();

    let link = flow.join("save/project");
    assert!(link.is_symlink(), "{}", link.display());
    assert_eq!(
        std::fs::canonicalize(&link).unwrap(),
        std::fs::canonicalize(&project).unwrap()
    );

    // A code stage: the link becomes the ticket's worktree, and writes go there.
    work.project_mount(&server, &worktree, ProjectMode::Worktree);
    assert_eq!(
        std::fs::canonicalize(&link).unwrap(),
        std::fs::canonicalize(&worktree).unwrap()
    );
    let file = SiteUrl::parse("site://flow/save/project/main.rs").unwrap();
    server.web.write_file(&file, b"fn main() {}").unwrap();
    assert!(worktree.join("main.rs").is_file());
    assert!(!project.join("main.rs").exists());

    // A read work: the same path, refused.
    work.project_mount(&server, &project, ProjectMode::Read);
    let refused = server.web.write_file(&file, b"x").unwrap_err();
    assert!(
        matches!(refused, composer_core::CoreError::MountRefused { .. }),
        "{refused:?}"
    );
    assert!(!project.join("main.rs").exists());

    // A write work: in place.
    work.project_mount(&server, &project, ProjectMode::Write);
    server.web.write_file(&file, b"fn main() {}").unwrap();
    assert!(project.join("main.rs").is_file());
}
