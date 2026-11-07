//! Home apps open with a token of their own (tickets 0168, 0170): `POST /api/apps/<site>/open` (or ws
//! `app.open`) gives `<base>/h/<site>/?app_token=...`; with it the app writes only its own site and
//! its Home data, has no project. The workspace's own route (`<base>/<site>/`) serves only client
//! apps (the user's `[apps] clients`, the chat app), opened with a one-time code that becomes the
//! cookie: the workspace token is never in an app's URL. A site's own `client: true` only asks:
//! unlisted, it opens with a token like any app. A project-only app doesn't open on Home.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::net::SocketAddr;
use std::path::Path;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use reqwest::{Client, Method, header};
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
    auth: Option<&str>,
    body: Option<Value>,
) -> reqwest::Response {
    let mut request = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .request(method, format!("http://{addr}{path}"));
    if let Some(token) = auth {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    request.send().await.unwrap()
}

async fn status_text(response: reqwest::Response) -> (u16, String) {
    (
        response.status().as_u16(),
        response.text().await.unwrap_or_default(),
    )
}

#[tokio::test]
async fn home_apps_have_their_own_tokens_and_clients_a_one_time_code() {
    let mut config = composer_server::Config::defaults();
    config.apps.clients = vec!["talk".into()];
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let sites = dir.path().join("sites");
    write(&sites.join("cal/index.md"), "---\ntitle: Cal\n---\n# Cal\n");
    write(&sites.join("cal/ui/dist/index.html"), "<p>cal</p>");
    write(
        &sites.join("talk/index.md"),
        "---\ntitle: Talk\napp:\n  client: true\n---\n# Talk\n",
    );
    write(&sites.join("talk/ui/dist/index.html"), "<p>talk</p>");
    // Asks to be a client; not listed.
    write(
        &sites.join("asker/index.md"),
        "---\ntitle: Asker\napp:\n  client: true\n---\n# Asker\n",
    );
    write(&sites.join("asker/ui/dist/index.html"), "<p>asker</p>");
    write(
        &sites.join("parts/index.md"),
        "---\ntitle: Parts\napp:\n  scope: project\n---\n# Parts\n",
    );
    let workspace = server.token.clone();

    // A plain Home app: its own token, under /h/.
    let (status, body) = status_text(
        call(
            addr,
            Method::POST,
            "/api/apps/cal/open",
            Some(&workspace),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let opened: Value = serde_json::from_str(&body).unwrap();
    let token = opened["token"].as_str().unwrap().to_string();
    assert_eq!(opened["url"], format!("/h/cal/?app_token={token}"));
    let page = call(
        addr,
        Method::GET,
        &format!("/h/cal/?app_token={token}"),
        None,
        None,
    )
    .await;
    let cookie = page.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie.contains("Path=/h/cal/"), "{cookie}");
    assert!(page.text().await.unwrap().contains("cal"));
    let me = Some(token.as_str());
    let note = json!({ "content": "---\ntitle: N\n---\nn\n" });
    let (status, _) = status_text(
        call(
            addr,
            Method::PUT,
            "/h/cal/api/pages/cal/n.md",
            me,
            Some(note.clone()),
        )
        .await,
    )
    .await;
    assert_eq!(status, 200);
    let (status, body) = status_text(
        call(
            addr,
            Method::PUT,
            "/h/cal/api/pages/talk/n.md",
            me,
            Some(note),
        )
        .await,
    )
    .await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("opened on Home"), "{body}");
    let (status, body) =
        status_text(call(addr, Method::GET, "/h/cal/api/project/files/x", me, None).await).await;
    assert_eq!(status, 403, "{body}");
    let (_, body) =
        status_text(call(addr, Method::GET, "/h/cal/api/project", me, None).await).await;
    assert_eq!(body, "null");
    let (status, _) = status_text(
        call(
            addr,
            Method::PUT,
            "/h/cal/api/data/view.json",
            me,
            Some(json!(1)),
        )
        .await,
    )
    .await;
    assert_eq!(status, 204);
    assert!(dir.path().join("state/home/cal/view.json").is_file());

    // The workspace's own route doesn't serve it.
    let (status, body) =
        status_text(call(addr, Method::GET, "/cal/", Some(&workspace), None).await).await;
    assert_eq!(status, 404);
    assert!(body.contains("a token of its own"), "{body}");

    // A client app: a one-time code, not the workspace token, in its URL.
    let (status, body) = status_text(
        call(
            addr,
            Method::POST,
            "/api/apps/talk/open",
            Some(&workspace),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let opened: Value = serde_json::from_str(&body).unwrap();
    let url = opened["url"].as_str().unwrap().to_string();
    assert!(url.starts_with("/talk/?once="), "{url}");
    assert!(!url.contains(&workspace) && opened.get("token").is_none());
    let page = call(addr, Method::GET, &url, None, None).await;
    assert_eq!(page.status(), 200);
    let cookie = page.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        cookie.starts_with(&format!("composer_token={workspace}")),
        "{cookie}"
    );
    // Once.
    assert_eq!(
        call(addr, Method::GET, &url, None, None).await.status(),
        404
    );

    // Asking isn't being: a token of its own, and the listing says it asks.
    let (status, body) = status_text(
        call(
            addr,
            Method::POST,
            "/api/apps/asker/open",
            Some(&workspace),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let opened: Value = serde_json::from_str(&body).unwrap();
    assert!(
        opened["url"]
            .as_str()
            .unwrap()
            .starts_with("/h/asker/?app_token=app_"),
        "{opened}"
    );
    let (status, _) =
        status_text(call(addr, Method::GET, "/asker/", Some(&workspace), None).await).await;
    assert_eq!(status, 404, "not served on the workspace's route");
    let (_, body) =
        status_text(call(addr, Method::GET, "/api/apps", Some(&workspace), None).await).await;
    let home: Value = serde_json::from_str(&body).unwrap();
    let errors = home["errors"].to_string();
    assert!(
        errors.contains("site://asker asks to be a client app"),
        "{errors}"
    );
    let client = |site: &str| {
        home["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["site"] == site)
            .unwrap()["client"]
            .as_bool()
            .unwrap()
    };
    assert!(client("talk") && !client("asker"));

    // A project app doesn't open on Home.
    let (status, body) = status_text(
        call(
            addr,
            Method::POST,
            "/api/apps/parts/open",
            Some(&workspace),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(status, 404);
    assert!(body.contains("open it from a project"), "{body}");

    // ws `app.open`.
    let mut socket = connect(addr, &workspace).await;
    send(
        &mut socket,
        1,
        ClientCommand::AppOpen {
            site: "cal".into(),
            project: None,
            session: None,
        },
    )
    .await;
    let events = until(&mut socket, |e| {
        matches!(
            e,
            ServerMessage::Ok { id: 1, .. } | ServerMessage::Error { id: Some(1), .. }
        )
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::AppOpened { opened },
        ..
    }) = events.last()
    else {
        panic!("{events:?}")
    };
    assert!(opened.url.starts_with("/h/cal/?app_token=app_"));
}
