//! The user's own local web servers through composer (`/apps/<name>/`): a fake app on loopback
//! behind the token, for the local user and a paired device; requests, an event stream that
//! arrives as it is sent, a POST body, a WebSocket echo, and none of composer's credentials
//! going upstream.

mod support;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::ws::{Message as UpMessage, WebSocketUpgrade};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use composer_protocol::http::{AppInfo, ServerInfo};
use composer_server::{AppConfig, Config, Server};
use futures::{SinkExt, StreamExt};
use reqwest::Client;
use support::*;
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

/// What the fake app saw of one request.
#[derive(Debug, Clone, Default)]
struct Seen {
    method: String,
    path: String,
    cookie: Option<String>,
    authorization: Option<String>,
    origin: Option<String>,
    referer: Option<String>,
    prefix: Option<String>,
    custom: Option<String>,
}

#[derive(Default)]
struct App {
    requests: Mutex<Vec<Seen>>,
    /// The event stream's second event waits for this.
    next_event: Notify,
}

type Shared = Arc<App>;

fn note(app: &App, headers: &HeaderMap, request: &Request) {
    let get = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    app.requests.lock().unwrap().push(Seen {
        method: request.method().to_string(),
        path: request
            .uri()
            .path_and_query()
            .map(ToString::to_string)
            .unwrap_or_default(),
        cookie: get("cookie"),
        authorization: get("authorization"),
        origin: get("origin"),
        referer: get("referer"),
        prefix: get("x-forwarded-prefix"),
        custom: get("x-custom"),
    });
}

async fn page(State(app): State<Shared>, headers: HeaderMap, request: Request) -> Response {
    note(&app, &headers, &request);
    let mut response = (
        [(header::CONTENT_TYPE, "text/html")],
        "<!doctype html><title>app</title><script src=\"app.js\"></script>",
    )
        .into_response();
    for cookie in ["composer_token_1=evil; Path=/", "board=3; Path=/"] {
        response
            .headers_mut()
            .append(header::SET_COOKIE, cookie.parse().unwrap());
    }
    response
}

/// One event now, the next once the test says so, then the end.
async fn events(State(app): State<Shared>, headers: HeaderMap, request: Request) -> Response {
    note(&app, &headers, &request);
    let stream = futures::stream::unfold(0, move |n| {
        let app = app.clone();
        async move {
            match n {
                0 => {}
                1 => app.next_event.notified().await,
                _ => return None,
            }
            let event = format!("data: {n}\n\n");
            Some((Ok::<_, std::io::Error>(event), n + 1))
        }
    });
    (
        [(header::CONTENT_TYPE, "text/event-stream")],
        Body::from_stream(stream),
    )
        .into_response()
}

async fn echo(State(app): State<Shared>, headers: HeaderMap, request: Request) -> Response {
    note(&app, &headers, &request);
    let method = request.method().to_string();
    let body = axum::body::to_bytes(request.into_body(), 16 << 20)
        .await
        .unwrap();
    (
        StatusCode::CREATED,
        format!(
            "{method} {} {}",
            body.len(),
            body[..body.len().min(5)].escape_ascii()
        ),
    )
        .into_response()
}

async fn redirect(State(app): State<Shared>, headers: HeaderMap, request: Request) -> Response {
    note(&app, &headers, &request);
    (StatusCode::FOUND, [(header::LOCATION, "/api/echo?x=1")]).into_response()
}

/// Echoes every message back; records the handshake.
async fn socket(
    State(app): State<Shared>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
    request: Request,
) -> Response {
    note(&app, &headers, &request);
    upgrade
        .protocols(["canvas.v1"])
        .on_upgrade(|mut socket| async move {
            while let Some(Ok(message)) = socket.recv().await {
                let back = match message {
                    UpMessage::Text(text) => UpMessage::Text(format!("echo {text}").into()),
                    UpMessage::Binary(data) => UpMessage::Binary(data),
                    UpMessage::Close(_) => break,
                    _ => continue,
                };
                if socket.send(back).await.is_err() {
                    break;
                }
            }
        })
}

async fn fake_app() -> (SocketAddr, Shared) {
    let app = Shared::default();
    let router = Router::new()
        .route("/", get(page))
        .route("/api/events", get(events))
        .route("/api/echo", any(echo))
        .route("/api/redirect", get(redirect))
        .route("/ws", get(socket))
        .route("/upload", post(echo))
        .with_state(app.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await });
    (addr, app)
}

fn app_config(upstream: SocketAddr) -> Config {
    let mut config = Config::defaults();
    config.apps.insert(
        "canvas".into(),
        AppConfig {
            url: format!("http://{upstream}"),
            title: "Canvas".into(),
        },
    );
    config
}

fn client() -> Client {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
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
async fn an_app_is_served_without_composers_credentials() {
    let (upstream, app) = fake_app().await;
    let (_dir, server, _p, addr) = start_with(app_config(upstream), vec![]).await;
    let token = server.token.clone();
    let base = format!("http://{addr}");

    let info: ServerInfo = client()
        .get(format!("{base}/api/info"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        info.apps,
        [AppInfo {
            name: "canvas".into(),
            title: "Canvas".into()
        }]
    );

    // Without a credential there is nothing; the bare name goes to the page, query kept.
    let response = client()
        .get(format!("{base}/apps/canvas/"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let response = client()
        .get(format!("{base}/apps/canvas?a=1"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 308);
    assert_eq!(response.headers()[header::LOCATION], "/apps/canvas/?a=1");
    let response = client()
        .get(format!("{base}/apps/nope/"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    assert!(response.text().await.unwrap().contains("[apps.nope]"));

    // The page, with every credential a browser could present, and the app's own cookie.
    let cookie_name = format!("composer_token_{}", addr.port());
    let response = client()
        .get(format!("{base}/apps/canvas/?token={token}&b=2"))
        .bearer_auth(&token)
        .header(
            header::COOKIE,
            format!("{cookie_name}={token}; board=3; composer_token=x"),
        )
        .header(header::ORIGIN, &base)
        .header(
            header::REFERER,
            format!("{base}/apps/canvas/?token={token}"),
        )
        .header("x-custom", "kept")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let set_cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    assert_eq!(set_cookies, ["board=3; Path=/"]);
    assert!(
        response
            .text()
            .await
            .unwrap()
            .contains("<title>app</title>")
    );

    // A redirect to the app's own path comes back under /apps/canvas.
    let response = client()
        .get(format!("{base}/apps/canvas/api/redirect"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 302);
    assert_eq!(
        response.headers()[header::LOCATION],
        "/apps/canvas/api/echo?x=1"
    );

    // Paths that would leave the app never reach it.
    for path in [
        "/apps/canvas/%2e%2e/x",
        "/apps/canvas//x",
        "/apps/canvas/a%2fb",
    ] {
        let response = client()
            .get(format!("{base}{path}"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 404, "{path}");
    }

    let seen = app.requests.lock().unwrap().clone();
    let paths: Vec<_> = seen.iter().map(|s| (&*s.method, &*s.path)).collect();
    assert_eq!(paths, [("GET", "/?b=2"), ("GET", "/api/redirect")]);
    let page = &seen[0];
    assert_eq!(page.cookie.as_deref(), Some("board=3"));
    assert_eq!(page.authorization, None);
    assert_eq!(page.origin, None);
    assert_eq!(page.referer, None);
    assert_eq!(page.prefix.as_deref(), Some("/apps/canvas"));
    assert_eq!(page.custom.as_deref(), Some("kept"));
}

#[tokio::test]
async fn events_bodies_and_sockets_go_through_for_a_paired_device() {
    let (upstream, app) = fake_app().await;
    let (_dir, server, _p, addr) = start_with(app_config(upstream), vec![]).await;
    let device = pair_device(&server, addr).await;
    let base = format!("http://{addr}");

    // The event stream: the first event arrives before the app sends the second.
    let response = client()
        .get(format!("{base}/apps/canvas/api/events"))
        .bearer_auth(&device)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    let mut stream = response.bytes_stream();
    let first = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("the first event before the stream ends")
        .unwrap()
        .unwrap();
    assert_eq!(&first[..], b"data: 0\n\n");
    app.next_event.notify_one();
    let second = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("the second event")
        .unwrap()
        .unwrap();
    assert_eq!(&second[..], b"data: 1\n\n");
    assert!(stream.next().await.is_none());

    // A POST body (a large one, streamed) and other methods.
    let big = vec![b'x'; 3 << 20];
    let response = client()
        .post(format!("{base}/apps/canvas/upload"))
        .bearer_auth(&device)
        .body(big)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    assert_eq!(
        response.text().await.unwrap(),
        format!("POST {} xxxxx", 3 << 20)
    );
    let chunks = futures::stream::iter(["he", "llo"].map(Ok::<_, std::io::Error>));
    let response = client()
        .put(format!("{base}/apps/canvas/api/echo"))
        .bearer_auth(&device)
        .body(reqwest::Body::wrap_stream(chunks))
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "PUT 5 hello");
    let response = client()
        .delete(format!("{base}/apps/canvas/api/echo"))
        .bearer_auth(&device)
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "DELETE 0 ");

    // A WebSocket, with the app's subprotocol, echoed both ways.
    let mut request = format!("ws://{addr}/apps/canvas/ws?token={device}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert(header::SEC_WEBSOCKET_PROTOCOL, "canvas.v1".parse().unwrap());
    request.headers_mut().insert(
        header::COOKIE,
        format!("composer_token_{}={device}; board=3", addr.port())
            .parse()
            .unwrap(),
    );
    let (mut ws, answer) = tokio_tungstenite::connect_async(request).await.unwrap();
    assert_eq!(
        answer.headers()[header::SEC_WEBSOCKET_PROTOCOL],
        "canvas.v1"
    );
    ws.send(Message::Text("hi".into())).await.unwrap();
    ws.send(Message::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    let mut back = Vec::new();
    for _ in 0..2 {
        let message = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("an echo")
            .unwrap()
            .unwrap();
        back.push(message);
    }
    assert_eq!(
        back,
        [
            Message::Text("echo hi".into()),
            Message::Binary(vec![1, 2, 3].into())
        ]
    );
    ws.send(Message::Close(None)).await.unwrap();

    let seen = app.requests.lock().unwrap().clone();
    let handshake = seen.iter().find(|s| s.path.starts_with("/ws")).unwrap();
    assert_eq!(handshake.path, "/ws");
    assert_eq!(handshake.cookie.as_deref(), Some("board=3"));
    assert_eq!(handshake.authorization, None);
    assert_eq!(handshake.origin, None);
    for request in &seen {
        assert_eq!(request.authorization, None, "{request:?}");
    }
}

#[tokio::test]
async fn an_app_that_is_not_running_is_a_bad_gateway() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let gone = listener.local_addr().unwrap();
    drop(listener);
    let (_dir, server, _p, addr) = start_with(app_config(gone), vec![]).await;
    let response = client()
        .get(format!("http://{addr}/apps/canvas/"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 502);
}
