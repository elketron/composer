//! An app messages the session it was opened from (ticket 0172): opened with `?session=` (or
//! `app.open { session }`), its messages go to that session and no other; opened outside a chat,
//! it messages none; an unknown session isn't opened from.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::net::SocketAddr;
use std::path::Path;

use reqwest::{Client, Method};
use serde_json::{Value, json};
use support::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

async fn call(
    addr: SocketAddr,
    method: Method,
    path: &str,
    token: &str,
    body: Option<Value>,
) -> (u16, String) {
    let mut request = Client::new()
        .request(method, format!("http://{addr}{path}"))
        .bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.unwrap();
    (
        response.status().as_u16(),
        response.text().await.unwrap_or_default(),
    )
}

#[tokio::test]
async fn an_app_messages_only_the_session_it_was_opened_from() {
    let (dir, server, _provider, addr) = start(vec![]).await;
    let sites = dir.path().join("sites");
    write(&sites.join("cal/index.md"), "---\ntitle: Cal\n---\n# Cal\n");
    write(&sites.join("cal/ui/dist/index.html"), "<p>cal</p>");
    write(
        &sites.join("cal/ui/dist/assets/a.js"),
        "d=\"composer-sdk:3\"",
    );
    let a = server.sessions.create("default", None).unwrap();
    let b = server.sessions.create("default", None).unwrap();
    let workspace = server.token.clone();
    let open = |session: Option<&str>| {
        let path = match session {
            Some(s) => format!("/api/apps/cal/open?session={s}"),
            None => "/api/apps/cal/open".to_string(),
        };
        let workspace = workspace.clone();
        async move { call(addr, Method::POST, &path, &workspace, None).await }
    };
    let token = |body: &str| {
        serde_json::from_str::<Value>(body).unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let message = json!({ "text": "from the app" });

    let (status, body) = open(Some(a.id())).await;
    assert_eq!(status, 200, "{body}");
    let from_a = token(&body);
    let to = |token: &str, session: &str| format!("/a/{token}/api/sessions/{session}/messages");
    let (status, body) = call(
        addr,
        Method::POST,
        &to(&from_a, a.id()),
        "",
        Some(message.clone()),
    )
    .await;
    assert_eq!(status, 202, "{body}");
    let (status, body) = call(
        addr,
        Method::POST,
        &to(&from_a, b.id()),
        "",
        Some(message.clone()),
    )
    .await;
    assert_eq!(status, 403, "{body}");
    assert!(
        body.contains(&format!("opened from session `{}`", a.id())),
        "{body}"
    );

    // Opened outside a chat: it messages none.
    let (_, body) = open(None).await;
    let nowhere = token(&body);
    let (status, body) = call(addr, Method::POST, &to(&nowhere, a.id()), "", Some(message)).await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("outside a chat"), "{body}");

    // An unknown session isn't opened from.
    let (status, body) = open(Some("nope")).await;
    assert_eq!(status, 404, "{body}");
    assert!(body.contains("no session `nope`"), "{body}");
}
