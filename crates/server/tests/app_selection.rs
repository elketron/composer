//! Stage → agent (ticket 0175): an app opened from a session points the agent at things. Its
//! selection becomes the session's chip (`selection.changed`), goes with the next user message
//! (once, unless cleared), and the `selection` tool reads it; its events reach the agent at the
//! next turn without starting one, at most 20 a minute; an app opened outside a chat can't.

mod support;

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use composer_protocol::{ClientCommand, ServerMessage};
use composer_server::provider::ModelEvent;
use reqwest::{Client, Method};
use serde_json::{Value, json};
use support::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

async fn post(addr: SocketAddr, path: &str, token: &str, body: Value) -> u16 {
    Client::new()
        .request(Method::POST, format!("http://{addr}{path}"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn open(addr: SocketAddr, workspace: &str, session: Option<&str>) -> String {
    let path = match session {
        Some(s) => format!("/api/apps/board/open?session={s}"),
        None => "/api/apps/board/open".into(),
    };
    let body: Value = Client::new()
        .post(format!("http://{addr}{path}"))
        .bearer_auth(workspace)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body["token"].as_str().unwrap().to_string()
}

fn answer() -> Vec<ModelEvent> {
    vec![ModelEvent::Content("ok".into()), finished(vec![])]
}

#[tokio::test]
async fn an_app_points_the_agent_at_things() {
    let (dir, server, provider, addr) = start(vec![answer(), answer()]).await;
    let sites = dir.path().join("sites");
    write(
        &sites.join("board/index.md"),
        "---\ntitle: Board\n---\n# Board\n",
    );
    write(&sites.join("board/ui/dist/index.html"), "<p>board</p>");
    write(
        &sites.join("board/ui/dist/assets/a.js"),
        "d=\"composer-sdk:2\"",
    );
    let session = server.sessions.create("default", None).unwrap();
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
    let app = open(addr, &server.token, Some(&id)).await;

    // Selecting: the chip.
    let status = post(
        addr,
        "/h/board/api/selection",
        &app,
        json!({ "label": "ticket 0042: fix the hinge", "url": "site://orchestration/tickets/0042.md", "text": "the hinge" }),
    )
    .await;
    assert_eq!(status, 204);
    let chip = loop {
        let e = tokio::time::timeout(Duration::from_secs(5), next_event(&mut client))
            .await
            .unwrap();
        if let ServerMessage::SelectionChanged { selection, .. } = e {
            break selection;
        }
    };
    assert_eq!(chip.unwrap().label, "ticket 0042: fix the hinge");
    assert_eq!(session.selection().unwrap().app, "board");
    // The agent's `selection` tool reads it.
    let context = composer_server::tools::ToolContext {
        server: server.clone(),
        session: session.clone(),
        agent: std::sync::Arc::new(
            composer_server::agent::AgentDefinition::load(&server.web, "default").unwrap(),
        ),
        cancel: tokio_util::sync::CancellationToken::new(),
    };
    let read = server
        .tools
        .call_value(&context, "selection", json!({}))
        .await;
    assert!(read.text.contains("ticket 0042"), "{}", read.text);

    // An event: no turn now; the agent sees it at the next one.
    let status = post(
        addr,
        "/h/board/api/events",
        &app,
        json!({ "kind": "gate.approved", "detail": { "stage": 2 } }),
    )
    .await;
    assert_eq!(status, 204);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        provider.requests().is_empty(),
        "an event doesn't start a turn"
    );

    // The next message carries the selection (once) and the event.
    send(
        &mut client,
        2,
        ClientCommand::MessageSend {
            session: id.clone(),
            text: "what about this?".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    for _ in 0..100 {
        if !provider.requests().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let first = format!("{:?}", provider.requests()[0].messages);
    assert!(
        first.contains("[Selected in the board app: ticket 0042: fix the hinge]")
            && first.contains("the hinge"),
        "{first}"
    );
    assert!(first.contains("the board app: gate.approved"), "{first}");
    assert!(session.selection().is_none(), "used once");

    // Cleared: the next message goes without it.
    post(
        addr,
        "/h/board/api/selection",
        &app,
        json!({ "label": "part 7" }),
    )
    .await;
    send(
        &mut client,
        3,
        ClientCommand::SelectionClear {
            session: id.clone(),
        },
    )
    .await;
    until(&mut client, |e| {
        matches!(e, ServerMessage::Ok { id: 3, .. })
    })
    .await;
    assert!(session.selection().is_none());

    // Opened outside a chat: it points at nothing. Too many events: refused.
    let nowhere = open(addr, &server.token, None).await;
    assert_eq!(
        post(
            addr,
            "/h/board/api/selection",
            &nowhere,
            json!({ "label": "x" })
        )
        .await,
        403
    );
    let mut statuses = Vec::new();
    for _ in 0..20 {
        statuses.push(post(addr, "/h/board/api/events", &app, json!({ "kind": "tick" })).await);
    }
    assert_eq!(
        statuses.iter().filter(|s| **s == 429).count(),
        1,
        "{statuses:?}"
    );
}
