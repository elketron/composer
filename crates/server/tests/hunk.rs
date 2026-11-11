//! The `hunk` tool: it reads a live review and its notes through Hunk's session CLI, replies,
//! comments, navigates, and exports a review page. A fake `hunk` stands in for the daemon.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use composer_server::agent::AgentDefinition;
use composer_server::tools::ToolContext;
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn fake_hunk(dir: &Path, repo: &Path) -> PathBuf {
    let repo = serde_json::to_string(&repo.display().to_string()).unwrap();
    let path = dir.join("hunk");
    let script = format!(
        r#"#!/bin/sh
case "$*" in
  "session list --json")
    cat <<EOF
{{"sessions":[{{"sessionId":"s1","repoRoot":{repo},"cwd":{repo},"title":"save working tree","fileCount":1,"files":[{{"path":"src/save.rs"}}],"snapshot":{{"state":{{"selectedFilePath":"src/save.rs"}}}}}}]}}
EOF
    ;;
  "session comment list --repo "*" --type user --json")
    cat <<'EOF'
{{"comments":[{{"noteId":"user:1","source":"user","filePath":"src/save.rs","newRange":[12,12],"body":"This can race."}}]}}
EOF
    ;;
  "session review --repo "*" --include-notes --json")
    cat <<'EOF'
{{"review":{{"title":"save working tree","repoRoot":"/p/save","selectedFile":{{"path":"src/save.rs"}},"selectedHunk":{{"header":"@@ -1,2 +1,3 @@"}},"files":[{{"path":"src/save.rs","additions":3,"deletions":1,"hunkCount":1}}],"reviewNotes":[{{"noteId":"user:1","source":"user","filePath":"src/save.rs","newRange":[12,12],"body":"This can race."}},{{"noteId":"mcp:2","source":"agent","filePath":"src/save.rs","newRange":[13,13],"body":"Fixed."}}]}}}}
EOF
    ;;
  "session comment add"*)
    echo '{{"result":{{"commentId":"mcp:3","body":"ok"}}}}'
    ;;
  "session navigate"*)
    echo '{{}}'
    ;;
  *)
    echo '{{}}'
    ;;
esac
"#
    );
    std::fs::write(&path, script).unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}

#[tokio::test]
async fn hunk_reads_notes_and_writes_back() {
    let projects = tempfile::tempdir().unwrap();
    let mut config = composer_server::Config::defaults();
    config.projects.dir = projects.path().display().to_string();
    let (dir, server, _provider, _addr) = start_with(config, vec![]).await;
    let project = projects.path().join("game");
    std::fs::create_dir_all(&project).unwrap();
    let project = project.canonicalize().unwrap();
    let bin = fake_hunk(dir.path(), &project);
    // SAFETY: this test is the only one that reads HUNK_BIN.
    unsafe { std::env::set_var("HUNK_BIN", &bin) };

    write(
        &dir.path().join("sites/flow/index.md"),
        "---\ntitle: Flow\ntype: index\n---\n# Flow\n",
    );
    server
        .write_page(
            &SiteUrl::new("agents", "hunker.md").unwrap(),
            "---\ntitle: hunker\ntype: agent\nname: hunker\ntools: [hunk]\nsites: all\n---\nHunks.\n",
            ChangedBy::External,
        )
        .unwrap();
    let session = server
        .sessions
        .create("hunker", Some(project.display().to_string()))
        .unwrap();
    let context = ToolContext {
        server: server.clone(),
        session,
        agent: std::sync::Arc::new(AgentDefinition::load(&server.web, "hunker").unwrap()),
        site: None,
        cancel: CancellationToken::new(),
    };
    let call = |args| {
        let (server, context) = (server.clone(), &context);
        async move { server.tools.call_value(context, "hunk", args).await }
    };

    let sessions = call(json!({ "action": "sessions" })).await;
    assert!(!sessions.is_error, "{}", sessions.text);
    assert!(
        sessions.text.contains("save working tree"),
        "{}",
        sessions.text
    );

    let notes = call(json!({ "action": "notes" })).await;
    assert!(!notes.is_error, "{}", notes.text);
    assert!(
        notes
            .text
            .contains("user:1 user `src/save.rs:12 (new)` — This can race."),
        "{}",
        notes.text
    );

    let review = call(json!({ "action": "review" })).await;
    assert!(!review.is_error, "{}", review.text);
    assert!(
        review
            .text
            .contains("selected: src/save.rs @@ -1,2 +1,3 @@")
            && review.text.contains("notes: 2 (1 user, 1 agent)"),
        "{}",
        review.text
    );

    let reply =
        call(json!({ "action": "reply", "note": "user:1", "text": "Fixed in the next commit." }))
            .await;
    assert!(!reply.is_error, "{}", reply.text);
    assert!(
        reply.text.contains("replied to note user:1"),
        "{}",
        reply.text
    );

    let comment =
        call(json!({ "action": "comment", "file": "src/save.rs", "line": 14, "text": "A note." }))
            .await;
    assert!(!comment.is_error, "{}", comment.text);
    assert!(
        comment.text.contains("added note mcp:3"),
        "{}",
        comment.text
    );

    let navigate = call(json!({ "action": "navigate", "note": "user:1" })).await;
    assert!(!navigate.is_error, "{}", navigate.text);

    let export = call(json!({
        "action": "export",
        "url": "site://flow/review-result.md"
    }))
    .await;
    assert!(!export.is_error, "{}", export.text);
    let page = std::fs::read_to_string(dir.path().join("sites/flow/review-result.md")).unwrap();
    assert!(
        page.contains("type: review")
            && page.contains("| `src/save.rs` | 3 | 1 | 1 |")
            && page.contains("This can race."),
        "{page}"
    );

    unsafe { std::env::remove_var("HUNK_BIN") };
}
