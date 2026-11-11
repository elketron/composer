//! wayai's viewer through composer (`<workspace>/view`): proxied to a fake upstream on loopback,
//! behind the workspace's token, view only. The fake serves a page, a WebSocket of binary frames
//! that records everything it receives, and an annotations endpoint that records its posts.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up fakes outside the sandbox"
)]

mod support;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::ws::{Message as UpMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use composer_protocol::http::{ServerInfo, SiteSummary};
use composer_server::apps::AppGrant;
use composer_server::{Config, Home, Host, Server, WorkspaceName, init_home, init_workspace};
use futures::{SinkExt, StreamExt};
use reqwest::Client;
use support::*;
use tokio_tungstenite::tungstenite::Message;

type Request1 = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// What the fake viewer saw.
#[derive(Default)]
struct Seen {
    /// `(method, path and query, cookie, authorization, host)` of each HTTP request.
    requests: Vec<Request1>,
    /// Bodies POSTed to the annotations endpoint.
    annotations: Vec<String>,
    /// Every message a WebSocket client sent it.
    received: Vec<String>,
    /// WebSocket connections that have ended on the upstream side.
    ws_ended: u32,
    /// `(authorization, origin)` of each WebSocket handshake.
    handshakes: Vec<(Option<String>, Option<String>)>,
    /// Where `/view/recordings` redirects to.
    redirect: String,
}

type Shared = Arc<Mutex<Seen>>;

async fn page(State(seen): State<Shared>, headers: HeaderMap, request: Request) -> Response {
    note(&seen, &headers, &request);
    (
        [
            (header::CONTENT_TYPE, "text/html"),
            (header::SET_COOKIE, "upstream=1"),
        ],
        "<!doctype html><title>fake viewer</title>",
    )
        .into_response()
}

async fn text(
    State(seen): State<Shared>,
    headers: HeaderMap,
    Path(seq): Path<u32>,
    request: Request,
) -> Response {
    note(&seen, &headers, &request);
    format!("boxes {seq}").into_response()
}

async fn annotate(State(seen): State<Shared>, headers: HeaderMap, request: Request) -> Response {
    note(&seen, &headers, &request);
    let body = axum::body::to_bytes(request.into_body(), 1 << 20)
        .await
        .unwrap();
    seen.lock()
        .unwrap()
        .annotations
        .push(String::from_utf8_lossy(&body).into_owned());
    (StatusCode::CREATED, "noted").into_response()
}

fn note(seen: &Shared, headers: &HeaderMap, request: &Request) {
    let get = |name| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    seen.lock().unwrap().requests.push((
        request.method().to_string(),
        request
            .uri()
            .path_and_query()
            .map(ToString::to_string)
            .unwrap_or_default(),
        get(header::COOKIE),
        get(header::AUTHORIZATION),
        get(header::HOST),
    ));
}

/// Binary frames until the client goes; with `?stop=1` three frames and then it closes.
async fn stream(
    State(seen): State<Shared>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
    request: Request,
) -> Response {
    let get = |name| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    seen.lock()
        .unwrap()
        .handshakes
        .push((get(header::AUTHORIZATION), get(header::ORIGIN)));
    let stop = request.uri().query().is_some_and(|q| q.contains("stop=1"));
    upgrade.on_upgrade(move |socket| frames(seen, socket, stop))
}

async fn frames(seen: Shared, socket: WebSocket, stop: bool) {
    let (mut tx, mut rx) = socket.split();
    let sender = tokio::spawn(async move {
        for n in 0u8.. {
            if tx
                .send(UpMessage::Binary(vec![0xFF, 0xD8, n].into()))
                .await
                .is_err()
            {
                return;
            }
            if stop && n == 2 {
                let _ = tx.send(UpMessage::Close(None)).await;
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    while let Some(Ok(message)) = rx.next().await {
        match message {
            UpMessage::Text(t) => seen.lock().unwrap().received.push(t.as_str().to_string()),
            UpMessage::Binary(b) => seen
                .lock()
                .unwrap()
                .received
                .push(format!("binary {}", b.len())),
            UpMessage::Close(_) => break,
            _ => {}
        }
    }
    sender.abort();
    seen.lock().unwrap().ws_ended += 1;
}

async fn redirect(State(seen): State<Shared>) -> Response {
    let to = seen.lock().unwrap().redirect.clone();
    (StatusCode::FOUND, [(header::LOCATION, to)]).into_response()
}

/// The fake viewer on an ephemeral loopback port.
async fn fake_viewer() -> (SocketAddr, Shared) {
    let seen = Shared::default();
    let app = Router::new()
        .route("/view", get(page))
        .route("/view/ws", get(stream))
        .route("/view/annotations", get(page).post(annotate))
        .route("/view/text/{seq}", get(text))
        .route("/view/other", get(page).post(annotate))
        .route("/view/recordings", get(redirect))
        .route("/view/recordings/{*rest}", get(page))
        // Not the viewer's: what a path that climbs out of `/view` would reach.
        .route("/mcp", get(page).post(annotate))
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (addr, seen)
}

fn viewer_config(upstream: SocketAddr) -> Config {
    let mut config = Config::defaults();
    config.viewer.url = format!("http://{upstream}/view");
    config
}

fn client() -> Client {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

#[tokio::test]
async fn pages_queries_and_annotations_are_forwarded_without_credentials() {
    let (upstream, seen) = fake_viewer().await;
    let (_dir, server, _p, addr) = start_with(viewer_config(upstream), vec![]).await;
    let token = server.token.clone();
    let base = format!("http://{addr}");
    assert_eq!(info(addr, &token).await.viewer.as_deref(), Some("/view"));

    // The page, with the token in the query and the cookie the browser would hold.
    let response = client()
        .get(format!("{base}/view?bare=1&token={token}"))
        .header(
            header::COOKIE,
            format!("composer_token_{}=other; theme=dark", addr.port()),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "text/html");
    let set_cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    assert!(
        set_cookies.iter().all(|c| !c.contains("upstream=1")),
        "{set_cookies:?}"
    );
    assert_eq!(
        response.text().await.unwrap(),
        "<!doctype html><title>fake viewer</title>"
    );

    // A sub-path with a query, authorized by a header and a cookie.
    let response = client()
        .get(format!("{base}/view/text/7?x=1&token={token}&y=two"))
        .bearer_auth(&token)
        .header(
            header::COOKIE,
            format!("composer_token_{}={token}", addr.port()),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "boxes 7");

    // An annotation is posted.
    let response = client()
        .post(format!("{base}/view/annotations?token={token}"))
        .json(&serde_json::json!({ "note": "here" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    assert_eq!(response.text().await.unwrap(), "noted");

    // Other methods, and a POST anywhere else, are refused and never reach the viewer.
    for (method, path) in [
        ("POST", "/view"),
        ("POST", "/view/other"),
        ("POST", "/view/text/1"),
        ("POST", "/view/ws"),
        ("PUT", "/view/annotations"),
        ("DELETE", "/view/annotations"),
    ] {
        let response = client()
            .request(method.parse().unwrap(), format!("{base}{path}"))
            .bearer_auth(&token)
            .body("x")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 405, "{method} {path}");
    }

    let seen = seen.lock().unwrap();
    assert_eq!(seen.annotations, [r#"{"note":"here"}"#]);
    let paths: Vec<_> = seen.requests.iter().map(|r| (&*r.0, &*r.1)).collect();
    assert_eq!(
        paths,
        [
            ("GET", "/view?bare=1"),
            ("GET", "/view/text/7?x=1&y=two"),
            ("POST", "/view/annotations"),
        ]
    );
    for (_, _, cookie, authorization, host) in &seen.requests {
        assert_eq!(cookie, &None);
        assert_eq!(authorization, &None);
        assert_eq!(host.as_deref(), Some(upstream.to_string().as_str()));
    }
}

async fn socket(addr: SocketAddr, path: &str, token: &str) -> Socket {
    let separator = if path.contains('?') { '&' } else { '?' };
    tokio_tungstenite::connect_async(format!("ws://{addr}{path}{separator}token={token}"))
        .await
        .unwrap()
        .0
}

/// Drive messages, as the viewer's own page sends them.
const DRIVE: [&str; 6] = [
    r#"{"t":"drive","on":true}"#,
    r#"{"t":"move","x":1,"y":2}"#,
    r#"{"t":"button","b":0,"down":true}"#,
    r#"{"t":"wheel","dx":0,"dy":3}"#,
    r#"{"t":"key","code":"KeyA","down":true}"#,
    r#"{"t":"release"}"#,
];

/// Reads three frames, then sends the Drive messages and a binary one.
async fn watch_and_send(ws: &mut Socket) {
    for _ in 0..3 {
        let message = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("a frame")
            .unwrap()
            .unwrap();
        let Message::Binary(frame) = message else {
            panic!("expected a binary frame, got {message:?}");
        };
        assert_eq!(&frame[..2], [0xFF, 0xD8]);
    }
    for text in DRIVE {
        ws.send(Message::Text(text.into())).await.unwrap();
    }
    ws.send(Message::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
}

async fn upstream_ends(seen: &Shared) {
    for _ in 0..100 {
        if seen.lock().unwrap().ws_ended == 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the upstream connection stayed open");
}

#[tokio::test]
async fn the_local_user_drives_through_the_socket() {
    let (upstream, seen) = fake_viewer().await;
    let (_dir, server, _p, addr) = start_with(viewer_config(upstream), vec![]).await;
    let mut ws = socket(addr, "/view/ws", &server.token).await;
    watch_and_send(&mut ws).await;
    // Text messages arrive in order; the binary one is not relayed.
    assert_eq!(seen.lock().unwrap().received, DRIVE);

    // Over the size limit the connection is closed and nothing of it is relayed.
    let big = "x".repeat(70 * 1024);
    let _ = ws.send(Message::Text(big.into())).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(seen.lock().unwrap().received.len(), DRIVE.len());
    upstream_ends(&seen).await;
}

#[tokio::test]
async fn a_paired_device_watches_and_nothing_goes_back() {
    let (upstream, seen) = fake_viewer().await;
    let (_dir, server, _p, addr) = start_with(viewer_config(upstream), vec![]).await;
    let device = pair_device(&server, addr).await;
    let mut ws = socket(addr, "/view/ws", &device).await;
    watch_and_send(&mut ws).await;
    assert_eq!(
        seen.lock().unwrap().received,
        Vec::<String>::new(),
        "the viewer must receive nothing"
    );
    // The client closing closes the upstream side.
    ws.send(Message::Close(None)).await.unwrap();
    upstream_ends(&seen).await;
}

async fn pair_device(server: &Server, addr: SocketAddr) -> String {
    let code = server.devices.create_code("phone").unwrap();
    let paired: serde_json::Value = client()
        .post(format!("http://{addr}/api/pair"))
        .json(&serde_json::json!({ "code": code, "name": "phone" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    paired["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn the_page_hides_drive_unless_the_caller_may_drive() {
    let (upstream, seen) = fake_viewer().await;
    let (_dir, server, _p, addr) = start_with(viewer_config(upstream), vec![]).await;
    let base = format!("http://{addr}");
    let device = pair_device(&server, addr).await;
    let get = |path: &str, token: &str| {
        client()
            .get(format!("{base}{path}"))
            .bearer_auth(token)
            .send()
    };
    get("/view?bare=1", &device).await.unwrap();
    get("/view", &device).await.unwrap();
    get("/view?drive=1&bare=1&x=2", &device).await.unwrap();
    get("/view?bare=1", &server.token).await.unwrap();
    get("/view", &server.token).await.unwrap();
    // Not the page: left alone.
    get("/view/text/3?a=1", &device).await.unwrap();
    // The device's cookie is a device too.
    let by_query = client()
        .get(format!("{base}/view?token={device}"))
        .send()
        .await
        .unwrap();
    assert_eq!(by_query.status(), 200);
    let paths: Vec<String> = seen
        .lock()
        .unwrap()
        .requests
        .iter()
        .map(|r| r.1.clone())
        .collect();
    assert_eq!(
        paths,
        [
            "/view?bare=1&drive=0",
            "/view?drive=0",
            "/view?bare=1&x=2&drive=0",
            "/view?bare=1",
            "/view",
            "/view/text/3?a=1",
            "/view?drive=0",
        ]
    );
}

#[tokio::test]
async fn the_upstream_closing_closes_the_client() {
    let (upstream, _seen) = fake_viewer().await;
    let (_dir, server, _p, addr) = start_with(viewer_config(upstream), vec![]).await;
    let mut ws = socket(addr, "/view/ws?stop=1", &server.token).await;
    let mut frames = 0;
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = ws.next().await {
            match message {
                Ok(Message::Binary(_)) => frames += 1,
                Ok(Message::Close(_)) | Err(_) => return,
                _ => {}
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the client socket never closed");
    assert_eq!(frames, 3);
}

#[tokio::test]
async fn only_the_workspaces_credentials_open_it() {
    let (upstream, _seen) = fake_viewer().await;
    let (dir, server, _p, addr) = start_with(viewer_config(upstream), vec![]).await;
    let base = format!("http://{addr}");

    // No credentials, a wrong one: refused like a route that doesn't exist.
    for path in ["/view", "/view/ws", "/view/text/1", "/view/annotations"] {
        let response = client().get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status(), 404, "{path}");
        let response = client()
            .get(format!("{base}{path}"))
            .bearer_auth("nope")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 404, "{path}");
    }
    assert!(
        tokio_tungstenite::connect_async(format!("ws://{addr}/view/ws"))
            .await
            .is_err()
    );

    // An app's own token opens nothing here, however it is presented.
    let app = server.app_tokens.mint(AppGrant {
        project: None,
        site: "cad".into(),
        local: None,
        session: None,
    });
    for request in [
        client().get(format!("{base}/view")).bearer_auth(&app),
        client().get(format!("{base}/view?token={app}")),
        client().get(format!("{base}/view")).header(
            header::COOKIE,
            format!("composer_token_{}={app}", addr.port()),
        ),
        client()
            .get(format!("{base}/view"))
            .header(header::COOKIE, format!("composer_app={app}")),
        client()
            .post(format!("{base}/view/annotations"))
            .bearer_auth(&app),
    ] {
        assert_eq!(request.send().await.unwrap().status(), 404);
    }
    assert!(
        tokio_tungstenite::connect_async(format!("ws://{addr}/view/ws?token={app}"))
            .await
            .is_err()
    );

    // The local token, its cookie and a paired device's token open it.
    assert_eq!(
        client()
            .get(format!("{base}/view"))
            .bearer_auth(&server.token)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let by_query = client()
        .get(format!("{base}/view?token={}", server.token))
        .send()
        .await
        .unwrap();
    assert_eq!(by_query.status(), 200);
    let cookie = by_query.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(
        cookie.starts_with(&format!("composer_token_{}=ses_", addr.port())),
        "{cookie}"
    );
    let cookie = cookie.split(';').next().unwrap().to_string();
    assert_eq!(
        client()
            .get(format!("{base}/view"))
            .header(header::COOKIE, cookie)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let code = server.devices.create_code("phone").unwrap();
    let paired: serde_json::Value = client()
        .post(format!("{base}/api/pair"))
        .json(&serde_json::json!({ "code": code, "name": "phone" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let device = paired["token"].as_str().unwrap();
    assert_eq!(
        client()
            .get(format!("{base}/view"))
            .bearer_auth(device)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let mut ws = socket(addr, "/view/ws", device).await;
    assert!(matches!(ws.next().await, Some(Ok(Message::Binary(_)))));
    drop(dir);
}

#[tokio::test]
async fn another_workspaces_credentials_do_not_open_it() {
    let (upstream, _seen) = fake_viewer().await;
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path().join("home"));
    init_home(&home).await.unwrap();
    init_workspace(&home.workspace(&WorkspaceName::new("b").unwrap()))
        .await
        .unwrap();
    let mut servers = BTreeMap::new();
    for name in home.workspace_names().unwrap() {
        let workspace = home.workspace(&name);
        let mut config = workspace.load_config().unwrap();
        config.viewer.url = format!("http://{upstream}/view");
        let provider = Arc::new(MockProvider::new(vec![]));
        servers.insert(name, Server::new(workspace, config, provider).unwrap());
    }
    let host = Arc::new(Host::new(home, servers).unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = host.clone();
    tokio::spawn(async move { serving.serve(listener).await });
    let get = |name: &str| {
        host.workspace(&WorkspaceName::new(name).unwrap())
            .unwrap()
            .clone()
    };
    let (a, b) = (get("default"), get("b"));
    for _ in 0..200 {
        if a.parent_url().is_some() && b.parent_url().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let status = |path: String, token: String| async move {
        client()
            .get(format!("http://{addr}{path}"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status()
    };
    for (own, other, name) in [(&a, &b, "default"), (&b, &a, "b")] {
        let view = format!("/w/{name}/view");
        assert_eq!(status(view.clone(), own.token.clone()).await, 200);
        assert_eq!(status(view.clone(), other.token.clone()).await, 404);
        assert_eq!(
            status(format!("{view}/text/1"), other.token.clone()).await,
            404
        );
        // A device paired to the other workspace.
        let code = other.devices.create_code("phone").unwrap();
        let device = other
            .devices
            .redeem(&code, Some("phone"))
            .unwrap()
            .unwrap()
            .1;
        assert_eq!(status(view.clone(), device).await, 404);
        // And a workspace's own socket opens only with its own token.
        let refused =
            tokio_tungstenite::connect_async(format!("ws://{addr}{view}/ws?token={}", other.token))
                .await;
        assert!(refused.is_err());
        let mut ws = socket(addr, &format!("{view}/ws"), &own.token).await;
        assert!(matches!(ws.next().await, Some(Ok(Message::Binary(_)))));
    }
}

#[tokio::test]
async fn no_viewer_configured_says_how_to() {
    let (_dir, server, _p, addr) = start(vec![]).await;
    let response = client()
        .get(format!("http://{addr}/view"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let message = response.text().await.unwrap();
    assert!(
        message.contains("[viewer]") && message.contains("[mcp.wayai]"),
        "{message}"
    );
    let response = client()
        .get(format!("http://{addr}/view/ws"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    assert_eq!(info(addr, &server.token).await.viewer, None);
}

async fn info(addr: SocketAddr, token: &str) -> ServerInfo {
    client()
        .get(format!("http://{addr}/api/info"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn the_viewer_is_taken_from_mcp_wayai() {
    let (upstream, seen) = fake_viewer().await;
    let mut config = Config::defaults();
    config.mcp.insert(
        "wayai".into(),
        toml_mcp(&format!("url = \"http://{upstream}/mcp\"")),
    );
    let (_dir, server, _p, addr) = start_with(config, vec![]).await;
    let response = client()
        .get(format!("http://{addr}/view?x=1"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(seen.lock().unwrap().requests[0].1, "/view?x=1");
    assert_eq!(
        info(addr, &server.token).await.viewer.as_deref(),
        Some("/view")
    );
}

fn toml_mcp(text: &str) -> composer_server::McpServerConfig {
    toml::from_str(text).unwrap()
}

#[tokio::test]
async fn a_non_loopback_upstream_is_no_viewer() {
    let mut config = Config::defaults();
    config.viewer.url = "http://192.168.1.20:7878/view".into();
    let (_dir, server, _p, addr) = start_with(config, vec![]).await;
    let response = client()
        .get(format!("http://{addr}/view"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let message = response.text().await.unwrap();
    assert!(message.contains("loopback"), "{message}");
    assert_eq!(info(addr, &server.token).await.viewer, None);
}

#[tokio::test]
async fn the_sites_listing_says_which_sites_are_workflows() {
    let (dir, server, _p, addr) = start(vec![]).await;
    let sites = dir.path().join("sites");
    for (name, index) in [
        (
            "flow",
            "---\ntitle: Flow\napp: blender\nmcp: [blender, comfy]\n---\n# Flow\n",
        ),
        ("mcponly", "---\ntitle: M\nmcp: wayai\n---\n# M\n"),
        ("plain", "---\ntitle: Plain\n---\n# Plain\n"),
    ] {
        std::fs::create_dir_all(sites.join(name)).unwrap();
        std::fs::write(sites.join(name).join("index.md"), index).unwrap();
    }
    let listing: Vec<SiteSummary> = client()
        .get(format!("http://{addr}/api/sites"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let site = |name: &str| listing.iter().find(|s| s.name == name).unwrap();
    assert_eq!(site("flow").app.as_deref(), Some("blender"));
    assert_eq!(site("flow").mcp, ["blender", "comfy"]);
    assert_eq!(site("mcponly").app, None);
    assert_eq!(site("mcponly").mcp, ["wayai"]);
    assert_eq!(site("plain").app, None);
    assert!(site("plain").mcp.is_empty());
    // A plain site's JSON has neither key.
    let raw: serde_json::Value = client()
        .get(format!("http://{addr}/api/sites"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let plain = raw
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "plain")
        .unwrap();
    assert!(plain.get("app").is_none() && plain.get("mcp").is_none());
}

/// `GET target` exactly as written (no client-side URL normalization), with a bearer token: the
/// status and the whole response as text.
async fn raw_get(addr: SocketAddr, target: &str, token: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "GET {target} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response).await;
    let response = String::from_utf8_lossy(&response).into_owned();
    let status = response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    (status, response)
}

#[tokio::test]
async fn a_path_that_would_leave_the_viewer_is_refused() {
    let (upstream, seen) = fake_viewer().await;
    let (_dir, server, _p, addr) = start_with(viewer_config(upstream), vec![]).await;
    let token = server.token.clone();

    // A plain path below the viewer goes through as it is.
    let (status, _) = raw_get(addr, "/view/recordings/a/b.webm?x=1", &token).await;
    assert_eq!(status, 200);
    assert_eq!(
        seen.lock().unwrap().requests[0].1,
        "/view/recordings/a/b.webm?x=1"
    );
    seen.lock().unwrap().requests.clear();

    let (status, response) = raw_get(addr, "/view/recordings/../../mcp", &token).await;
    assert_eq!(status, 404, "{response}");
    for target in [
        "/view/recordings/../../mcp",
        "/view/recordings/..",
        "/view/recordings/a/../../../mcp",
        "/view/recordings/%2e%2e/%2e%2e/mcp",
        "/view/recordings/%2E%2E/%2e%2E/mcp",
        "/view/recordings/.%2e/%2e./mcp",
        "/view/recordings/..%2f..%2fmcp",
        "/view/recordings/..%2F..%2Fmcp",
        "/view/recordings/a%5c..%5C..%5cmcp",
        "/view/recordings/./a",
        "/view/recordings//a",
        "/view/recordings/a/",
        "/view/text/..",
        "/view/text/%2e%2e",
        "/view/text/..%2fmcp",
    ] {
        let (status, response) = raw_get(addr, target, &token).await;
        assert_eq!(status, 404, "{target}: {response}");
        assert!(!response.contains("fake viewer"), "{target}: {response}");
    }
    // A raw backslash may already be refused by the HTTP layer; it never gets through.
    let (status, response) = raw_get(addr, "/view/recordings/a\\..\\..\\mcp", &token).await;
    assert!(status == 404 || status == 400, "{status}: {response}");
    assert_eq!(
        seen.lock().unwrap().requests,
        Vec::<Request1>::new(),
        "nothing reached the upstream"
    );
}

#[tokio::test]
async fn an_encoded_drive_parameter_does_not_ride_along() {
    let (upstream, seen) = fake_viewer().await;
    let (_dir, server, _p, addr) = start_with(viewer_config(upstream), vec![]).await;
    let device = pair_device(&server, addr).await;
    for target in [
        "/view?%64rive=1&bare=1",
        "/view?dri%76e=1&d%52ive=x&drive=1",
    ] {
        assert_eq!(raw_get(addr, target, &device).await.0, 200, "{target}");
    }
    assert_eq!(raw_get(addr, "/view?%64rive=1", &server.token).await.0, 200);
    let paths: Vec<String> = seen
        .lock()
        .unwrap()
        .requests
        .iter()
        .map(|r| r.1.clone())
        .collect();
    assert_eq!(
        paths,
        [
            "/view?bare=1&drive=0",
            "/view?d%52ive=x&drive=0",
            "/view?%64rive=1"
        ]
    );
}

#[tokio::test]
async fn a_redirect_stays_on_the_viewer_or_loses_its_target() {
    let (upstream, seen) = fake_viewer().await;
    let (_dir, server, _p, addr) = start_with(viewer_config(upstream), vec![]).await;
    for (to, location) in [
        ("/view/recordings/a?x=1", Some("/view/recordings/a?x=1")),
        (
            &format!("http://{upstream}/view/text/3"),
            Some("/view/text/3"),
        ),
        ("//evil.example/view", None),
        ("/\\evil.example/view", None),
        ("http://evil.example/view", None),
        ("https://evil.example/view/recordings/a", None),
        ("/mcp", None),
        ("/view/../mcp", None),
    ] {
        seen.lock().unwrap().redirect = to.to_string();
        let response = client()
            .get(format!("http://{addr}/view/recordings"))
            .bearer_auth(&server.token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 302, "{to}");
        let got = response
            .headers()
            .get(header::LOCATION)
            .map(|v| v.to_str().unwrap());
        assert_eq!(got, location, "{to}");
    }
}

/// A WebSocket to `path` authenticated by `cookie` alone.
async fn socket_with_cookie(addr: SocketAddr, path: &str, cookie: &str) -> Socket {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request = format!("ws://{addr}{path}").into_client_request().unwrap();
    request
        .headers_mut()
        .insert(header::COOKIE, cookie.parse().unwrap());
    tokio_tungstenite::connect_async(request).await.unwrap().0
}

#[tokio::test]
async fn a_session_drives_only_when_the_local_user_opened_it() {
    let (upstream, seen) = fake_viewer().await;
    let (_dir, server, _p, addr) = start_with(viewer_config(upstream), vec![]).await;
    let device = pair_device(&server, addr).await;
    // The cookie a browser gets for a token in the URL: a session of that token's kind.
    let session = |token: String| async move {
        let response = client()
            .get(format!("http://{addr}/api/info?token={token}"))
            .send()
            .await
            .unwrap();
        let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(!cookie.contains(&token), "{cookie}");
        cookie.split(';').next().unwrap().to_string()
    };

    let mut ws = socket_with_cookie(addr, "/view/ws", &session(device).await).await;
    watch_and_send(&mut ws).await;
    assert_eq!(
        seen.lock().unwrap().received,
        Vec::<String>::new(),
        "a device's session is a device: the viewer must receive nothing"
    );
    ws.send(Message::Close(None)).await.unwrap();
    upstream_ends(&seen).await;

    let mut ws = socket_with_cookie(addr, "/view/ws", &session(server.token.clone()).await).await;
    watch_and_send(&mut ws).await;
    assert_eq!(seen.lock().unwrap().received, DRIVE);
}

#[tokio::test]
async fn the_viewers_bearer_token_goes_upstream_and_nowhere_else() {
    let (upstream, seen) = fake_viewer().await;
    let files = tempfile::tempdir().unwrap();
    let file = files.path().join("token");
    std::fs::write(&file, "viewer-secret-1\n").unwrap();
    let mut config = viewer_config(upstream);
    config.viewer.bearer_file = file.display().to_string();
    let (_dir, server, _p, addr) = start_with(config, vec![]).await;
    let base = format!("http://{addr}");
    let upstream_saw = |seen: &Shared| -> Vec<Option<String>> {
        seen.lock()
            .unwrap()
            .requests
            .iter()
            .map(|r| r.3.clone())
            .collect()
    };

    // HTTP: the viewer's token replaces whatever the client presented, and never comes back.
    let response = client()
        .get(format!("{base}/view?bare=1"))
        .bearer_auth(&server.token)
        .header(header::COOKIE, "upstream=1")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let returned = format!("{:?}", response.headers());
    assert!(!returned.contains("viewer-secret"), "{returned}");
    assert!(!response.text().await.unwrap().contains("viewer-secret"));
    let posted = client()
        .post(format!("{base}/view/annotations"))
        .bearer_auth(&server.token)
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(posted.status(), 201);
    let bearer = Some("Bearer viewer-secret-1".to_string());
    assert_eq!(upstream_saw(&seen), [bearer.clone(), bearer.clone()]);

    // The WebSocket handshake carries it too, and no Origin.
    let mut ws = socket(addr, "/view/ws", &server.token).await;
    assert!(matches!(ws.next().await, Some(Ok(Message::Binary(_)))));
    assert_eq!(seen.lock().unwrap().handshakes, [(bearer, None)]);
    drop(ws);

    // Read for every request: a new token (the viewer restarted) is the one sent next.
    std::fs::write(&file, "viewer-secret-2").unwrap();
    client()
        .get(format!("{base}/view/text/1"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    assert_eq!(
        upstream_saw(&seen).last().unwrap().as_deref(),
        Some("Bearer viewer-secret-2")
    );

    // A file that cannot be read: a clear failure naming it, and nothing is sent without it.
    std::fs::remove_file(&file).unwrap();
    let before = upstream_saw(&seen).len();
    for path in ["/view", "/view/ws", "/view/text/1"] {
        let response = client()
            .get(format!("{base}{path}"))
            .bearer_auth(&server.token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 502, "{path}");
        let message = response.text().await.unwrap();
        assert!(
            message.contains(&file.display().to_string()) && message.contains("cannot read"),
            "{message}"
        );
    }
    assert!(
        tokio_tungstenite::connect_async(format!("ws://{addr}/view/ws?token={}", server.token))
            .await
            .is_err()
    );
    assert_eq!(upstream_saw(&seen).len(), before);
    assert_eq!(seen.lock().unwrap().handshakes.len(), 1);
}

#[tokio::test]
async fn without_a_bearer_file_no_authorization_goes_upstream() {
    let (upstream, seen) = fake_viewer().await;
    let (_dir, server, _p, addr) = start_with(viewer_config(upstream), vec![]).await;
    client()
        .get(format!("http://{addr}/view"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    let mut ws = socket(addr, "/view/ws", &server.token).await;
    assert!(matches!(ws.next().await, Some(Ok(Message::Binary(_)))));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.requests[0].3, None);
    assert_eq!(seen.handshakes, [(None, None)]);
}
