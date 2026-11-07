//! Agent → stage (ticket 0174): `show_app`, `show_page` and `desk` put tabs on the session's desk
//! for the agent: unseen, the shown tab unchanged, `desk.changed` naming what it opened, so a
//! following client shows it and another badges it. An app must be one of the project's. The
//! project home lists its apps' tiles from their data.

mod support;

use std::sync::Arc;
use std::time::Duration;

use composer_core::SiteUrl;
use composer_protocol::{ChangedBy, ClientCommand, DeskBy, ServerMessage};
use composer_server::Config;
use composer_server::agent::AgentDefinition;
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
    let mut config = Config::defaults();
    config.projects.dir = root.display().to_string();
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let sites = dir.path().join("sites");
    write(
        &sites.join("cad/index.md"),
        "---\ntitle: CAD\napp:\n  scope: project\n  when: [\"files:*.FCStd\"]\n  tile: true\n---\n# CAD\n",
    );
    write(&sites.join("cal/index.md"), "---\ntitle: Cal\n---\n# Cal\n");
    std::fs::create_dir_all(sites.join("cal/ui")).unwrap();
    write(
        &sites.join("other/index.md"),
        "---\ntitle: Other\napp:\n  scope: project\n---\n# Other\n",
    );
    server
        .write_page(
            &SiteUrl::new("agents", "stager.md").unwrap(),
            "---\ntitle: stager\ntype: agent\nname: stager\ntools: [show_app, show_page, desk]\nsites: all\n---\nShows.\n",
            ChangedBy::External,
        )
        .unwrap();
    let session = server
        .sessions
        .create("stager", Some(root.join("game").display().to_string()))
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
        session: session.clone(),
        agent: Arc::new(AgentDefinition::load(&server.web, "stager").unwrap()),
        cancel: CancellationToken::new(),
    };
    let tool = |name: &'static str, args: Value| {
        let (server, context) = (server.clone(), &context);
        async move { server.tools.call_value(context, name, args).await }
    };

    let shown = tool("show_app", json!({ "site": "cad" })).await;
    assert!(
        !shown.is_error && shown.text.contains("app:cad@game"),
        "{}",
        shown.text
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
    assert_eq!(event, (DeskBy::Agent, Some("app:cad@game".to_string())));

    let refused = tool("show_app", json!({ "site": "other" })).await;
    assert!(
        refused.is_error && refused.text.contains("isn't one of project `game`'s apps"),
        "{}",
        refused.text
    );
    let home = tool("show_app", json!({ "site": "cal", "home": true })).await;
    assert!(home.text.contains("app:cal"), "{}", home.text);
    let page = tool("show_page", json!({ "url": "site://cad" })).await;
    assert!(page.text.contains("page:site://cad"), "{}", page.text);
    let changes = tool("desk", json!({ "changes": true })).await;
    assert!(changes.text.contains("changes:game"), "{}", changes.text);
    let listed = tool("desk", json!({})).await;
    assert!(
        listed.text.contains("app:cad@game (not looked at yet)"),
        "{}",
        listed.text
    );

    // The user shows it: seen.
    send(
        &mut client,
        2,
        ClientCommand::DeskFocus {
            session: id.clone(),
            tab: "app:cad@game".into(),
        },
    )
    .await;
    until(&mut client, |e| {
        matches!(e, ServerMessage::Ok { id: 2, .. })
    })
    .await;
    let listed = tool("desk", json!({})).await;
    assert!(
        listed.text.contains("app:cad@game (shown)"),
        "{}",
        listed.text
    );
    let pointed = tool("desk", json!({ "point": "page:site://cad" })).await;
    assert!(!pointed.is_error, "{}", pointed.text);

    // The project home: the app's tile from its data for the project.
    write(
        &dir.path().join("state/projects/game/cad/tile.md"),
        "2 parts changed\n",
    );
    let response: Value = reqwest::Client::new()
        .get(format!("http://{addr}/api/projects/game/home"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(response["apps"][0]["site"], "cad");
    assert_eq!(response["apps"][0]["tile_text"], "2 parts changed");
}
