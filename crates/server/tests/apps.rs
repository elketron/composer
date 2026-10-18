//! Micro apps: scaffolding, serving with cookie auth, files API, and messages from apps.

mod support;

use composer_protocol::{ClientCommand, CommandResult, MessageSource, Role, ServerMessage};
use composer_server::provider::{ModelEvent, ModelMessage};
use serde_json::json;
use support::*;

fn no_redirects() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

#[tokio::test]
async fn new_app_scaffolds_the_ui_and_a_build_action() {
    let (dir, server, _p, addr) = start(vec![
        vec![finished(vec![tool_call(
            "n",
            "new_app",
            json!({ "site": "plans" }),
        )])],
        vec![ModelEvent::Content("done".into()), finished(vec![])],
    ])
    .await;
    assert!(
        dir.path().join("templates/sdk/index.ts").is_file(),
        "init installs the sdk"
    );
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
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: session.id.clone(),
            since: None,
        },
    )
    .await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session: session.id.clone(),
            text: "app please".into(),
            pages: vec![],
            images: vec![],
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(events.iter().any(|e| matches!(e, ServerMessage::ToolResult { is_error: false, result, .. } if result.contains("created site://plans/ui"))));

    let ui = dir.path().join("sites/plans/ui");
    let package = std::fs::read_to_string(ui.join("package.json")).unwrap();
    assert!(package.contains("\"name\": \"plans-ui\""), "{package}");
    assert!(
        std::fs::read_to_string(ui.join("index.html"))
            .unwrap()
            .contains("<title>plans</title>")
    );
    assert!(ui.join("src/App.svelte").is_file());
    let justfile = std::fs::read_to_string(dir.path().join("sites/plans/justfile")).unwrap();
    assert!(
        justfile.contains("ui-build:\n    cd ui && bun install && bun run build"),
        "{justfile}"
    );
}

#[tokio::test]
async fn apps_are_served_with_cookie_auth() {
    let (dir, server, _p, addr) = start(vec![]).await;
    let dist = dir.path().join("sites/plans/ui/dist");
    std::fs::create_dir_all(dist.join("assets")).unwrap();
    std::fs::write(
        dist.join("index.html"),
        "<!doctype html><title>plans</title>",
    )
    .unwrap();
    std::fs::write(dist.join("assets/app.js"), "console.log(1)").unwrap();
    let base = format!("http://{addr}");
    let client = no_redirects();

    // /plans?token=… → /plans/?token=… (keeps the query).
    let redirect = client
        .get(format!("{base}/plans?token={}&session=ab12", server.token))
        .send()
        .await
        .unwrap();
    assert_eq!(redirect.status(), 308);
    assert_eq!(
        redirect.headers()["location"],
        format!("/plans/?token={}&session=ab12", server.token)
    );

    // Opening with the token sets the cookie.
    let index = client
        .get(format!("{base}/plans/?token={}", server.token))
        .send()
        .await
        .unwrap();
    assert_eq!(index.status(), 200);
    let cookie = index.headers()["set-cookie"].to_str().unwrap().to_string();
    assert!(
        cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"),
        "{cookie}"
    );
    let cookie = cookie.split(';').next().unwrap().to_string();

    // Assets and the API work with the cookie alone, not without it.
    let asset = client
        .get(format!("{base}/plans/assets/app.js"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(asset.status(), 200);
    assert!(
        asset.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/javascript")
    );
    assert_eq!(
        client
            .get(format!("{base}/plans/assets/app.js"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .get(format!("{base}/api/sites"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // Client-side routes fall back to index.html; traversal is refused.
    let route = client
        .get(format!("{base}/plans/some/route"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(
        route.text().await.unwrap(),
        "<!doctype html><title>plans</title>"
    );
    let escape = client
        .get(format!("{base}/plans/..%2F..%2F..%2Fstate%2Ftoken"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert!(escape.status().is_client_error(), "{}", escape.status());

    // A site without a built app explains itself.
    let missing = client
        .get(format!("{base}/memory/"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
    assert!(missing.text().await.unwrap().contains("ui-build"));
}

#[tokio::test]
async fn files_api_round_trips_bytes_and_notifies() {
    let (dir, server, _p, addr) = start(vec![]).await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::Subscribe {
            urls: vec!["site://plans/**".into()],
        },
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;

    let png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0, 1, 2, 255];
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/api/files/plans/mockups/login/snapshot.png");
    let put = client
        .put(&url)
        .bearer_auth(&server.token)
        .body(png.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 200);
    assert_eq!(
        std::fs::read(dir.path().join("sites/plans/mockups/login/snapshot.png")).unwrap(),
        png
    );

    let get = client
        .get(&url)
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    assert_eq!(get.headers()["content-type"], "image/png");
    assert_eq!(get.bytes().await.unwrap().to_vec(), png);

    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::PageChanged { .. })
    })
    .await;
    assert!(
        matches!(events.last(), Some(ServerMessage::PageChanged { by: composer_protocol::ChangedBy::App, url, .. })
        if url == "site://plans/mockups/login/snapshot.png")
    );
}

#[tokio::test]
async fn an_app_message_reaches_the_agent_marked_with_its_site() {
    let (_dir, server, provider, addr) = start(vec![vec![
        ModelEvent::Content("looking".into()),
        finished(vec![]),
    ]])
    .await;
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
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: session.id.clone(),
            since: None,
        },
    )
    .await;

    let response = reqwest::Client::new()
        .post(format!("http://{addr}/api/sessions/{}/messages", session.id))
        .bearer_auth(&server.token)
        .json(&json!({ "site": "design", "text": "box 1: make this a dropdown", "pages": ["site://design/mockups/login"], "images": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(events.iter().any(|e| matches!(e, ServerMessage::MessageDone { message, .. }
        if message.role == Role::User && message.source == Some(MessageSource::App { site: "design".into() }))));
    let request = &provider.requests()[0];
    let Some(ModelMessage::User { content, .. }) = request.messages.last() else {
        panic!()
    };
    assert!(
        content.starts_with("[from the design app]")
            && content.contains("- site://design/mockups/login"),
        "{content}"
    );
}
