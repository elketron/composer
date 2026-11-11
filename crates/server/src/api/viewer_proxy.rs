//! wayai's live view for other devices: `<workspace>/view` and what the viewer serves under it,
//! forwarded to the loopback upstream (`[viewer]`, see [`ViewerUpstream`]).
//!
//! Behind the workspace token like every route here (the layer in `api/mod.rs`); a micro app's
//! token never reaches it. Who may drive the application is one rule, [`Credential::may_drive`]:
//! only the machine's own user (the workspace's local token). For that caller the WebSocket also
//! relays the text messages the client sends (the viewer page's Drive mode) to wayai, in order and
//! within a size limit; for everyone else (a paired device) the proxy is view-only: frames go to
//! the client and everything it sends is dropped, and the page is asked for without its Drive
//! control (`drive=0`, a display option of wayai's page; the relay rule is what enforces it). The
//! only POST forwarded is the annotations one. Nothing of the client's credentials goes upstream
//! and nothing of the upstream's cookies comes back. What goes upstream as a credential is the
//! viewer's own bearer token (`[viewer] bearer_file`), read from its file for every request and
//! handshake; it never reaches the client, and no `Origin` is sent. A path that would leave the
//! viewer is refused before any URL is built ([`clean_rest`]).
//!
//! `/view/<site>/<path>` is also the read-only markdown view of a site (`view_routes.rs`), so only
//! the paths the viewer is known to serve are taken here (see `router`); a site named `ws`,
//! `annotations`, `text` or `recordings` is not viewable that way.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequest, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::api::frame_queue::FrameQueue;
use crate::api::{ApiError, Credential};
use crate::server::Server;
use crate::viewer_upstream::ViewerUpstream;

/// Frames waiting for a slow client; the oldest go first.
const QUEUE_FRAMES: usize = 3;
/// The largest message a client may send over the socket.
const CLIENT_MESSAGE_LIMIT: usize = 64 * 1024;
/// A POST (an annotation) larger than this is refused.
const POST_LIMIT: usize = 8 * 1024 * 1024;
/// Parameters that are composer's own, never the viewer's.
const OWN_PARAMS: [&str; 2] = ["token", "once"];

/// What the client may send, from the client's request to the upstream.
const FORWARDED_REQUEST_HEADERS: [HeaderName; 6] = [
    header::ACCEPT,
    header::ACCEPT_LANGUAGE,
    header::CONTENT_TYPE,
    header::RANGE,
    header::IF_NONE_MATCH,
    header::IF_MODIFIED_SINCE,
];
/// What comes back from the upstream (no `Set-Cookie`).
const FORWARDED_RESPONSE_HEADERS: [HeaderName; 9] = [
    header::CONTENT_TYPE,
    header::CONTENT_LENGTH,
    header::CONTENT_RANGE,
    header::CONTENT_DISPOSITION,
    header::ACCEPT_RANGES,
    header::CACHE_CONTROL,
    header::ETAG,
    header::LAST_MODIFIED,
    header::CONTENT_LANGUAGE,
];

/// The viewer's routes; merged into the token-protected router.
pub fn router() -> Router<Arc<Server>> {
    Router::new()
        .route("/view", get(http))
        .route("/view/", get(http))
        .route("/view/ws", get(socket))
        .route("/view/annotations", get(http).post(http))
        .route("/view/text/{seq}", get(http))
        .route("/view/recordings", get(http))
        .route("/view/recordings/{*rest}", get(http))
}

/// Why the live view can't be opened.
pub(crate) const NO_LIVE_VIEW: &str =
    "this workspace has no live view: `[viewer]` or `[mcp.wayai]` is not configured in config.toml";

/// What `GET /api/info` says: the path after the workspace's address, when there is a viewer.
pub fn viewer_path(server: &Server) -> Option<String> {
    ViewerUpstream::from_config(&server.config())
        .ok()
        .flatten()
        .map(|_| "/view".to_string())
}

/// The upstream, or the 404 that says how to get one.
fn upstream(server: &Server) -> Result<ViewerUpstream, ApiError> {
    match ViewerUpstream::from_config(&server.config()) {
        Ok(Some(upstream)) => Ok(upstream),
        Ok(None) => Err(no_viewer(
            "no viewer: set `[viewer] url = \"http://127.0.0.1:7878/view\"` in config.toml, or \
             configure `[mcp.wayai]` with a `url`",
        )),
        Err(why) => Err(no_viewer(&format!("no viewer: {why}"))),
    }
}

fn no_viewer(message: &str) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, message.to_string())
}

/// Whether the request was authenticated as the machine's own user. A request without a
/// credential (it never gets here: the layer refuses it) may not drive.
fn may_drive(request: &Request) -> bool {
    request
        .extensions()
        .get::<Credential>()
        .is_some_and(|credential| credential.may_drive())
}

/// One request for the upstream: where it goes and the bearer token the viewer asks for.
struct Target {
    url: String,
    bearer: Option<HeaderValue>,
}

/// The upstream request for this one: `rest` under the viewer (`""`, `/ws`, `/text/3`), checked
/// ([`clean_rest`]), the query without composer's parameters, and the viewer's bearer token read
/// from its file now.
fn target(server: &Server, request: &Request, ws: bool) -> Result<Target, ApiError> {
    let upstream = upstream(server)?;
    let not_found = || ApiError(StatusCode::NOT_FOUND, "not found".to_string());
    let rest = request
        .uri()
        .path()
        .strip_prefix("/view")
        .filter(|rest| clean_rest(rest))
        .ok_or_else(not_found)?;
    let query = upstream_query(request.uri().query(), rest, may_drive(request));
    let url = upstream.url(rest, Some(&query), ws).ok_or_else(not_found)?;
    let unusable = |why: String| {
        tracing::warn!("viewer upstream: {why}");
        ApiError(
            StatusCode::BAD_GATEWAY,
            format!("the viewer's token: {why}"),
        )
    };
    let bearer = upstream
        .bearer()
        .map_err(unusable)?
        .map(|token| {
            HeaderValue::from_str(&format!("Bearer {token}")).map(|mut value| {
                value.set_sensitive(true);
                value
            })
        })
        .transpose()
        .map_err(|_| unusable("the bearer token file does not hold a token (one line)".into()))?;
    Ok(Target { url, bearer })
}

/// Whether `rest` (the request's path below `/view`, as the client sent it) is one the proxy
/// forwards: empty or `/` (the page), or `/segment/...` with no empty, `.` or `..` segment, no
/// backslash, and no percent-encoded dot, slash or backslash (`%2e`, `%2f`, `%5c`, any case).
/// The URL parser would resolve those and the request would leave the viewer
/// (`/view/recordings/../../mcp`).
fn clean_rest(rest: &str) -> bool {
    if matches!(rest, "" | "/") {
        return true;
    }
    let lower = rest.to_ascii_lowercase();
    let Some(segments) = rest.strip_prefix('/') else {
        return false;
    };
    !rest.contains('\\')
        && !["%2e", "%2f", "%5c"].iter().any(|bad| lower.contains(bad))
        && segments
            .split('/')
            .all(|segment| !matches!(segment, "" | "." | ".."))
}

/// The query for the upstream: composer's own parameters removed; and for the page (`rest` empty
/// or `/`) of a client that may not drive, `drive=0` (replacing any `drive`), the viewer page's
/// option to hide its Drive control. Names are compared percent-decoded, as the viewer will read
/// them (`%64rive=1` is `drive=1`). Other parameters stay as they were.
fn upstream_query(query: Option<&str>, rest: &str, may_drive: bool) -> String {
    let hide_drive = !may_drive && matches!(rest, "" | "/");
    let mut pairs: Vec<&str> = query
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let key = percent_decoded(pair.split_once('=').map_or(*pair, |(key, _)| key));
            let ours = OWN_PARAMS.contains(&key.as_str()) || (hide_drive && key == "drive");
            !ours
        })
        .collect();
    if hide_drive {
        pairs.push("drive=0");
    }
    pairs.join("&")
}

/// A query parameter's name as a server reads it: `%XX` decoded, `+` a space.
fn percent_decoded(name: &str) -> String {
    let bytes = name.as_bytes();
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let (mut out, mut at) = (Vec::with_capacity(bytes.len()), 0);
    while at < bytes.len() {
        let pair = bytes
            .get(at + 1)
            .zip(bytes.get(at + 2))
            .and_then(|(high, low)| Some(hex(*high)? * 16 + hex(*low)?));
        match (bytes[at], pair) {
            (b'%', Some(byte)) => {
                out.push(byte);
                at += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                at += 1;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("an http client builds")
    })
}

async fn http(State(server): State<Arc<Server>>, request: Request) -> Response {
    let Target { url, bearer } = match target(&server, &request, false) {
        Ok(target) => target,
        Err(refused) => return refused.into_response(),
    };
    let (parts, body) = request.into_parts();
    // Only the annotations endpoint takes a body; the router already refuses other methods.
    if parts.method != Method::GET && parts.method != Method::POST && parts.method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let mut upstream_request = client().request(parts.method.clone(), &url);
    for name in &FORWARDED_REQUEST_HEADERS {
        if let Some(value) = parts.headers.get(name) {
            upstream_request = upstream_request.header(name, value);
        }
    }
    // The viewer's own token, from composer's config: never the client's, never sent back.
    if let Some(bearer) = bearer {
        upstream_request = upstream_request.header(header::AUTHORIZATION, bearer);
    }
    if parts.method == Method::POST {
        match axum::body::to_bytes(body, POST_LIMIT).await {
            Ok(bytes) => upstream_request = upstream_request.body(bytes),
            Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        }
    }
    let upstream_response = match upstream_request.send().await {
        Ok(response) => response,
        Err(e) => return bad_gateway(&e.to_string()),
    };
    let status = upstream_response.status();
    let headers = upstream_response.headers().clone();
    let mut response = Response::new(Body::from_stream(upstream_response.bytes_stream()));
    *response.status_mut() = status;
    copy_headers(&headers, response.headers_mut());
    if let Some(location) = headers.get(header::LOCATION)
        && let Some(path) = location.to_str().ok().and_then(|l| {
            ViewerUpstream::from_config(&server.config())
                .ok()
                .flatten()?
                .local_path(l)
        })
        && let Ok(value) = HeaderValue::from_str(&format!("{}{path}", server.route_base()))
    {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response
}

fn copy_headers(from: &HeaderMap, to: &mut HeaderMap) {
    for name in &FORWARDED_RESPONSE_HEADERS {
        for value in from.get_all(name) {
            to.append(name.clone(), value.clone());
        }
    }
}

fn bad_gateway(why: &str) -> Response {
    tracing::debug!("viewer upstream: {why}");
    (
        StatusCode::BAD_GATEWAY,
        "the viewer is not reachable (is wayai running?)",
    )
        .into_response()
}

/// `GET /view/ws`: the upstream's frames to the client; what the client sends goes back only
/// when it may drive.
async fn socket(State(server): State<Arc<Server>>, request: Request) -> Response {
    let Target { url, bearer } = match target(&server, &request, true) {
        Ok(target) => target,
        Err(refused) => return refused.into_response(),
    };
    let drive = may_drive(&request);
    let upgrade = match WebSocketUpgrade::from_request(request, &()).await {
        Ok(upgrade) => upgrade,
        Err(rejected) => return rejected.into_response(),
    };
    // The handshake carries the viewer's token and no `Origin` (the viewer refuses a foreign one).
    let mut handshake = match url.into_client_request() {
        Ok(handshake) => handshake,
        Err(e) => return bad_gateway(&e.to_string()),
    };
    if let Some(bearer) = bearer {
        handshake
            .headers_mut()
            .insert(header::AUTHORIZATION, bearer);
    }
    let upstream = match tokio_tungstenite::connect_async(handshake).await {
        Ok((upstream, _)) => upstream,
        Err(e) => return bad_gateway(&e.to_string()),
    };
    upgrade
        // Drive messages are small; the limit keeps a client's messages from costing memory.
        .max_message_size(CLIENT_MESSAGE_LIMIT)
        .on_upgrade(move |client| relay(client, upstream, drive))
}

type Upstream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Frames go upstream → client through a short queue that drops the oldest. Of the client's
/// messages, with `drive` its text messages (Drive: moves, buttons, keys) go to the upstream in
/// the order they came; everything else, and all of it without `drive`, is dropped except its
/// close. Either side closing ends both.
async fn relay(client: WebSocket, upstream: Upstream, drive: bool) {
    let (mut to_client, mut from_client) = client.split();
    let (mut to_upstream, mut from_upstream) = upstream.split();
    let frames = Arc::new(FrameQueue::<Message>::new(QUEUE_FRAMES));

    let queue = frames.clone();
    let reader = tokio::spawn(async move {
        while let Some(Ok(message)) = from_upstream.next().await {
            match message {
                tungstenite::Message::Binary(data) => {
                    queue.push(Message::Binary(data.to_vec().into()))
                }
                tungstenite::Message::Text(text) => queue.push(Message::Text(text.as_str().into())),
                tungstenite::Message::Close(_) => break,
                // Pings are answered by the library.
                _ => {}
            }
        }
        queue.close();
    });
    let mut writer = tokio::spawn(async move {
        while let Some(message) = frames.pop().await {
            if to_client.send(message).await.is_err() {
                return;
            }
        }
        let _ = to_client
            .send(Message::Close(Some(CloseFrame {
                code: 1000,
                reason: "".into(),
            })))
            .await;
    });

    // Keeps the library's queued pongs going out while the client says nothing.
    let mut keepalive = tokio::time::interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            message = from_client.next() => match message {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(Message::Text(text))) if drive => {
                    let text = tungstenite::Message::Text(text.as_str().into());
                    if to_upstream.send(text).await.is_err() {
                        break;
                    }
                }
                // Everything else is dropped, not relayed.
                Some(Ok(_)) => {}
            },
            _ = &mut writer => break,
            _ = keepalive.tick() => {
                if to_upstream.flush().await.is_err() {
                    break;
                }
            }
        }
    }
    reader.abort();
    writer.abort();
    let _ = to_upstream.send(tungstenite::Message::Close(None)).await;
    let _ = to_upstream.close().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_paths_below_the_viewer() {
        for rest in ["", "/", "/ws", "/text/3", "/recordings/a/b.webm", "/a%20b"] {
            assert!(clean_rest(rest), "{rest}");
        }
        for rest in [
            "/recordings/../../mcp",
            "/..",
            "/.",
            "/recordings/./a",
            "/recordings//a",
            "/recordings/",
            "//",
            "/%2e%2e/mcp",
            "/%2E%2e/mcp",
            "/a%2fb",
            "/a%2Fb",
            "/a%5cb",
            "/a%5Cb",
            "/a\\b",
            "/.%2e/mcp",
            "x",
        ] {
            assert!(!clean_rest(rest), "{rest}");
        }
    }

    #[test]
    fn drive_is_found_however_its_name_is_written() {
        assert_eq!(
            upstream_query(Some("a=1&drive=1"), "", false),
            "a=1&drive=0"
        );
        assert_eq!(
            upstream_query(Some("%64rive=1&a=1"), "/", false),
            "a=1&drive=0"
        );
        assert_eq!(
            upstream_query(Some("dri%76e=1&%44rive=1&d%72ive"), "", false),
            "%44rive=1&drive=0",
            "names are case-sensitive: `Drive` is another parameter"
        );
        assert_eq!(upstream_query(Some("%64rive=1"), "", true), "%64rive=1");
        assert_eq!(upstream_query(Some("drive=1"), "/text/3", false), "drive=1");
        assert_eq!(
            upstream_query(Some("%74oken=x&o%6Ece=y&b=%7"), "", true),
            "b=%7"
        );
        assert_eq!(percent_decoded("a+b%zz%4"), "a b%zz%4");
    }
}
