//! A `type: tui` page opens its program in a terminal on the session's desk, in the project (or
//! the work's `project/` view). The hunk control adapter is tested separately.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use composer_protocol::DeskTab;
use composer_server::agent::AgentDefinition;
use composer_server::tools::ToolContext;
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

fn write(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[tokio::test]
async fn a_tui_page_starts_in_the_project() {
    if std::process::Command::new("hunk")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("hunk is not installed; skipping");
        return;
    }
    let projects = tempfile::tempdir().unwrap();
    let mut config = composer_server::Config::defaults();
    config.projects.dir = projects.path().display().to_string();
    let (dir, server, _provider, _addr) = start_with(config, vec![]).await;
    let project = projects.path().join("game");
    std::fs::create_dir_all(&project).unwrap();
    write(&project.join("README.md"), "# game\n");
    write(
        &dir.path().join("sites/flow/index.md"),
        "---\ntitle: Flow\ntype: index\n---\n# Flow\n",
    );
    write(
        &dir.path().join("sites/flow/review.md"),
        "---\ntitle: Review\ntype: tui\ncommand: hunk diff\ncontrol: hunk\ncwd: project\n---\n# Review\n",
    );

    let session = server
        .sessions
        .create("default", Some(project.display().to_string()))
        .unwrap();
    let context = ToolContext {
        server: server.clone(),
        session: session.clone(),
        agent: std::sync::Arc::new(AgentDefinition::load(&server.web, "default").unwrap()),
        site: None,
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
