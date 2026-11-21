//! A `type: tui` page opens its program in a terminal on the session's desk, in the folder of
//! the session's site. The hunk control adapter is tested separately.

mod support;

use composer_protocol::DeskTab;
use composer_server::tools::ToolContext;
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

fn write(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[tokio::test]
async fn a_tui_page_starts_in_the_sites_folder() {
    if std::process::Command::new("hunk")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("hunk is not installed; skipping");
        return;
    }
    let projects = tempfile::tempdir().unwrap();
    let (dir, server, _provider, _addr) = start(vec![]).await;
    let project = projects.path().join("game");
    std::fs::create_dir_all(&project).unwrap();
    write(&project.join("README.md"), "# game\n");
    write(
        &dir.path().join("sites/flow/index.md"),
        &format!(
            "---\ntitle: Flow\ntype: index\nfolder: {}\n---\n# Flow\n",
            project.display()
        ),
    );
    write(
        &dir.path().join("sites/flow/review.md"),
        "---\ntitle: Review\ntype: tui\ncommand: hunk diff\ncontrol: hunk\ncwd: folder\n---\n# Review\n",
    );

    let session = server
        .sessions
        .create(
            composer_protocol::RunKind::Agentic,
            None,
            Some("flow".into()),
        )
        .unwrap();
    let context = ToolContext {
        server: server.clone(),
        run: server.sink(&session),
        kind: composer_protocol::RunKind::Agentic,
        only: None,
        site: Some(std::sync::Arc::new(
            composer_server::agent::Site::load(&server.web, "flow").unwrap(),
        )),
        cancel: CancellationToken::new(),
    };
    let opened = server
        .tools
        .call_value(&context, "open", json!({ "url": "site://flow/review" }))
        .await;
    assert!(!opened.is_error, "{}", opened.text);
    assert!(
        opened.text.contains("started `hunk diff`"),
        "{}",
        opened.text
    );

    let id = session
        .desk()
        .tabs
        .iter()
        .find_map(|tab| match &tab.tab {
            DeskTab::Terminal { id } => Some(id.clone()),
            _ => None,
        })
        .expect("a terminal tab");
    let terminal = server.terminals.get(&id).expect("the terminal");
    assert_eq!(
        terminal.cwd.canonicalize().unwrap(),
        project.canonicalize().unwrap()
    );
    server.terminals.close(&server, &id).unwrap();
}
