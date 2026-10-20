//! The changes view's data: git diff of the active project.

mod support;

use std::path::Path;
use std::process::Command;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::project::project_diff;
use support::*;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    std::fs::write(dir.path().join("lib.rs"), "fn a() {}\nfn b() {}\n").unwrap();
    std::fs::write(dir.path().join("old.rs"), "gone\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    dir
}

#[tokio::test]
async fn modified_deleted_and_untracked_files() {
    let dir = repo();
    std::fs::write(
        dir.path().join("lib.rs"),
        "fn a() {}\nfn b2() {}\nfn c() {}\n",
    )
    .unwrap();
    std::fs::remove_file(dir.path().join("old.rs")).unwrap();
    std::fs::write(dir.path().join("new.rs"), "fn new() {}\n").unwrap();

    let diff = project_diff(dir.path(), None).await.unwrap();
    assert_eq!(diff.branch.as_deref(), Some("main"));
    let files: Vec<(&str, &str, u32, u32)> = diff
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.status.as_str(), f.added, f.removed))
        .collect();
    assert_eq!(
        files,
        [
            ("lib.rs", "M", 2, 1),
            ("old.rs", "D", 0, 1),
            ("new.rs", "?", 1, 0)
        ]
    );
    assert!(
        diff.diff.contains("+fn b2() {}") && diff.diff.contains("-fn b() {}"),
        "{}",
        diff.diff
    );
    assert!(
        diff.diff
            .contains("+++ b/new.rs\n@@ -0,0 +1,1 @@\n+fn new() {}"),
        "{}",
        diff.diff
    );
}

#[tokio::test]
async fn repository_without_commits() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    std::fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    let diff = project_diff(dir.path(), None).await.unwrap();
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].status, "A");
    assert!(diff.diff.contains("+hello"));
}

#[tokio::test]
async fn not_a_repository() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        project_diff(dir.path(), None)
            .await
            .unwrap_err()
            .contains("not a git checkout")
    );
}

#[tokio::test]
async fn endpoint_uses_the_session_project() {
    let project = repo();
    std::fs::write(project.path().join("lib.rs"), "changed\n").unwrap();
    let (_dir, server, _p, addr) = start(vec![]).await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            agent: "default".into(),
            project: None,
        },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Session { session },
        ..
    }) = reply.last()
    else {
        panic!()
    };

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/api/sessions/{}/project/diff", session.id);
    assert_eq!(
        client
            .get(&url)
            .bearer_auth(&server.token)
            .send()
            .await
            .unwrap()
            .status(),
        404,
        "no project yet"
    );

    let path = project.path().display().to_string();
    send(
        &mut socket,
        2,
        ClientCommand::ProjectSet {
            session: session.id.clone(),
            path: Some(path),
        },
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 2, .. })
    })
    .await;
    let diff: serde_json::Value = client
        .get(&url)
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(diff["files"][0]["path"], "lib.rs");
    assert!(diff["diff"].as_str().unwrap().contains("+changed"));
}
