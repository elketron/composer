//! Agent → stage (ticket 0174): `show_page` and `desk` put tabs on the session's desk for the
//! agent: unseen, the shown tab unchanged, `desk.changed` naming what it opened, so a following
//! client shows it and another badges it.

mod support;

use std::time::Duration;

use composer_protocol::{ClientCommand, DeskBy, ServerMessage};
use composer_server::Config;
use composer_server::tools::ToolContext;
use serde_json::{Value, json};
use support::*;
use tokio_util::sync::CancellationToken;

fn write(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[tokio::test]
async fn the_agent_puts_things_on_the_stage() {
    let projects = tempfile::tempdir().unwrap();
    let root = projects.path().canonicalize().unwrap();
    write(&root.join("game/arm.FCStd"), "");
    let (dir, server, _provider, addr) = start_with(Config::defaults(), vec![]).await;
    let sites = dir.path().join("sites");
    write(&sites.join("cad/index.md"), "---\ntitle: CAD\n---\n# CAD\n");
    write(
        &sites.join("game/index.md"),
        &format!(
            "---\ntitle: Game\nfolder: {}\n---\n# Game\n",
            root.join("game").display()
        ),
    );
    approve_folder(&server, "game");
    let session = server
        .sessions
        .create(
            composer_protocol::RunKind::Agentic,
            None,
            Some("game".into()),
        )
        .unwrap();
    let id = session.id().to_string();
    let mut client = connect(addr, &server.token).await;
    send(
        &mut client,
        1,
        ClientCommand::SessionAttach {
            session: id.clone(),
            since: None,
        },
    )
    .await;
    until(&mut client, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let context = ToolContext {
        server: server.clone(),
        run: server.sink(&session),
        kind: composer_protocol::RunKind::Agentic,
        only: None,
        site: Some(std::sync::Arc::new(
            composer_server::agent::Site::load(&server.web, "game").unwrap(),
        )),
        cancel: CancellationToken::new(),
    };
    let tool = |name: &'static str, args: Value| {
        let (server, context) = (server.clone(), &context);
        async move { server.tools.call_value(context, name, args).await }
    };

    let page = tool("show_page", json!({ "url": "site://cad" })).await;
    assert!(
        !page.is_error && page.text.contains("page:site://cad"),
        "{}",
        page.text
    );
    // Unseen, and the shown tab didn't move; the client is told what was opened.
    let desk = session.desk();
    assert!(desk.tabs[0].unseen && desk.active.is_none(), "{desk:?}");
    let event = loop {
        let e = tokio::time::timeout(Duration::from_secs(5), next_event(&mut client))
            .await
            .unwrap();
        if let ServerMessage::DeskChanged { by, opened, .. } = e {
            break (by, opened);
        }
    };
    assert_eq!(event, (DeskBy::Agent, Some("page:site://cad".to_string())));

    let changes = tool("desk", json!({ "changes": true })).await;
    assert!(changes.text.contains("changes:game"), "{}", changes.text);
    let listed = tool("desk", json!({})).await;
    assert!(
        listed.text.contains("page:site://cad (not looked at yet)"),
        "{}",
        listed.text
    );

    // The user shows it: seen.
    send(
        &mut client,
        2,
        ClientCommand::DeskFocus {
            session: id.clone(),
            tab: "page:site://cad".into(),
        },
    )
    .await;
    until(&mut client, |e| {
        matches!(e, ServerMessage::Ok { id: 2, .. })
    })
    .await;
    let listed = tool("desk", json!({})).await;
    assert!(
        listed.text.contains("page:site://cad (shown)"),
        "{}",
        listed.text
    );
    let pointed = tool("desk", json!({ "point": "changes:game" })).await;
    assert!(!pointed.is_error, "{}", pointed.text);
}

#[tokio::test]
async fn the_agent_puts_the_live_view_on_the_stage_when_there_is_one() {
    let mut with = Config::defaults();
    with.viewer.url = "http://127.0.0.1:9/view".into();
    for (config, viewer) in [(Config::defaults(), false), (with, true)] {
        let (_dir, server, _provider, _addr) = start_with(config, vec![]).await;
        let session = server
            .sessions
            .create(composer_protocol::RunKind::Agentic, None, None)
            .unwrap();
        let context = ToolContext {
            server: server.clone(),
            run: server.sink(&session),
            kind: composer_protocol::RunKind::Agentic,
            only: None,
            site: None,
            cancel: CancellationToken::new(),
        };
        let result = server
            .tools
            .call_value(&context, "desk", json!({ "live": true }))
            .await;
        if viewer {
            assert!(
                !result.is_error && result.text.contains("live"),
                "{}",
                result.text
            );
            let desk = session.desk();
            assert!(desk.tabs[0].unseen && desk.active.is_none(), "{desk:?}");
        } else {
            assert!(
                result.is_error && result.text.contains("[viewer]"),
                "{}",
                result.text
            );
            assert!(session.desk().tabs.is_empty());
        }
    }
}

#[tokio::test]
async fn the_agent_puts_a_local_app_on_the_stage() {
    let mut config = Config::defaults();
    config.apps.insert(
        "canvas".into(),
        composer_server::AppConfig {
            url: "http://127.0.0.1:9".into(),
            title: "Canvas".into(),
        },
    );
    let (_dir, server, _provider, _addr) = start_with(config, vec![]).await;
    let session = server
        .sessions
        .create(composer_protocol::RunKind::Agentic, None, None)
        .unwrap();
    let context = ToolContext {
        server: server.clone(),
        run: server.sink(&session),
        kind: composer_protocol::RunKind::Agentic,
        only: None,
        site: None,
        cancel: CancellationToken::new(),
    };
    let tool = |name: &'static str, args: Value| {
        let (server, context) = (server.clone(), &context);
        async move { server.tools.call_value(context, name, args).await }
    };

    let listed = tool("desk", json!({})).await;
    assert!(
        listed.text.contains("app://canvas/ (Canvas)"),
        "{}",
        listed.text
    );
    let shown = tool("show_page", json!({ "url": "app://canvas/board/2" })).await;
    assert!(
        !shown.is_error && shown.text.contains("app:canvas"),
        "{}",
        shown.text
    );
    let desk = session.desk();
    assert_eq!(
        desk.tabs[0].tab,
        composer_protocol::DeskTab::App {
            name: "canvas".into(),
            path: "board/2".into()
        }
    );
    assert!(desk.tabs[0].unseen);
    let unknown = tool("show_page", json!({ "url": "app://nope/" })).await;
    assert!(
        unknown.is_error && unknown.text.contains("app://canvas/"),
        "{}",
        unknown.text
    );
    let climbing = tool("show_page", json!({ "url": "app://canvas/../x" })).await;
    assert!(climbing.is_error, "{}", climbing.text);
    assert_eq!(session.desk().tabs.len(), 1);
}
