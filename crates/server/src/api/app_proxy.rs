//! The user's own local web servers (`[apps.<name>]`) for every client: `/apps/<name>/...`
//! forwarded to the app ([`AppUpstream`]): any method, the request's body and the response's
//! streamed as they come (an event stream stays open with no timeout), WebSockets relayed both
//! ways.
//!
//! Behind the workspace token like every route here (the layer in `api/mod.rs`): the machine's
//! own user and a paired device alike. Nothing of the client's credentials goes upstream (no
//! `Authorization`, no `?token=`/`?once=`, no composer cookie, no `Origin` or `Referer`), and no
//! upstream cookie named like composer's comes back. A path that would leave the app is refused
//! before any URL is built ([`crate::app_upstream::clean_rest`]). `/apps/<name>` redirects to
//! `/apps/<name>/`, so the app's relative URLs resolve under it.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequest, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::api::ApiError;
use crate::api::auth::COOKIE;
use crate::api::viewer_proxy::percent_decoded;
use crate::app_upstream::AppUpstream;
use crate::server::Server;

/// Parameters that are composer's own, never the app's.
const OWN_PARAMS: [&str; 2] = ["token", "once"];

/// Request headers that stay here: hop-by-hop ones, the client's credentials and where it came
/// from. `Cookie` goes, without composer's ([`app_cookies`]).
const DROPPED_REQUEST_HEADERS: [&str; 16] = [
    "host",
    "connection",
    "keep-alive",
    "proxy-connection",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "authorization",
    "cookie",
    "origin",
    "referer",
    // The body comes back as the app sent it; composer adds no decoding.
    "accept-encoding",
    "forwarded",
    "x-real-ip",
];
/// Response headers that stay there (hop-by-hop). `Set-Cookie` comes back, without composer's.
const DROPPED_RESPONSE_HEADERS: [&str; 7] = [
    "connection",
    "keep-alive",
    "proxy-connection",
    "proxy-authenticate",
    "te",
    "trailer",
    "transfer-encoding",
];

/// The apps' routes; merged into the token-protected router.
pub fn router() -> Router<Arc<Server>> {
    Router::new()
        .route("/apps/{name}", any(to_page))
        .route("/apps/{name}/", any(forward))
        .route("/apps/{name}/{*rest}", any(forward))
}

/// What a client is told when `name` is not configured.
pub(crate) fn no_app(name: &str) -> String {
    format!("no app `{name}`: add `[apps.{name}] url = \"http://127.0.0.1:<port>\"` to config.toml")
}

/// The app the request's path names, and the rest of the path after it (`/`, `/api/x`).
fn upstream(server: &Server, request: &Request) -> Result<(AppUpstream, String), ApiError> {
    let path = request.uri().path();
    let below = path
        .strip_prefix(crate::app_upstream::SERVED_AT)
        .and_then(|p| p.strip_prefix('/'))
        .unwrap_or_default();
    let (name, rest) = match below.split_once('/') {
        Some((name, rest)) => (name, format!("/{rest}")),
        None => (below, String::new()),
    };
    let app = AppUpstream::find(&server.config(), name)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, no_app(name)))?;
    Ok((app, rest))
}

/// `/apps/<name>`: to `/apps/<name>/`, the query kept.
async fn to_page(State(server): State<Arc<Server>>, request: Request) -> Response {
    let (app, _) = match upstream(&server, &request) {
        Ok(found) => found,
        Err(refused) => return refused.into_response(),
    };
    let location = match request.uri().query() {
        Some(query) => format!("{}/?{query}", app.served_at()),
        None => format!("{}/", app.served_at()),
    };
    (
        StatusCode::PERMANENT_REDIRECT,
        [(header::LOCATION, location)],
    )
        .into_response()
}

/// The query for the app: composer's own parameters removed (compared percent-decoded, as a
/// server reads them), the rest as they were.
fn upstream_query(query: Option<&str>) -> String {
    query
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let key = percent_decoded(pair.split_once('=').map_or(*pair, |(key, _)| key));
            !OWN_PARAMS.contains(&key.as_str())
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// The client's cookies without composer's (any `composer_token*`, so no other port's either);
/// `None` when nothing is left.
fn app_cookies(headers: &HeaderMap) -> Option<HeaderValue> {
    let kept: Vec<&str> = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .map(str::trim)
        .filter(|pair| !pair.is_empty() && !is_composer_cookie(pair))
        .collect();
    (!kept.is_empty())
        .then(|| HeaderValue::from_str(&kept.join("; ")).ok())
        .flatten()
}

/// Whether a `name=value` (a `Cookie` pair, or a `Set-Cookie`'s start) is composer's auth cookie.
fn is_composer_cookie(pair: &str) -> bool {
    pair.split_once('=')
        .map_or(pair, |(name, _)| name)
        .trim()
        .starts_with(COOKIE)
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // No overall timeout: an event stream stays open as long as the app keeps it.
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("an http client builds")
    })
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn bad_gateway(app: &AppUpstream, why: &str) -> Response {
    tracing::debug!("app `{}`: {why}", app.name());
    (
        StatusCode::BAD_GATEWAY,
        format!("the app `{}` is not reachable (is it running?)", app.name()),
    )
        .into_response()
}

async fn forward(State(server): State<Arc<Server>>, request: Request) -> Response {
    let (app, rest) = match upstream(&server, &request) {
        Ok(found) => found,
        Err(refused) => return refused.into_response(),
    };
    let websocket = request
        .headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    let query = upstream_query(request.uri().query());
    let Some(url) = app.url(&rest, Some(&query), websocket) else {
        return not_found();
    };
    if websocket {
        return socket(app, url, request).await;
    }
    let (parts, body) = request.into_parts();
    let mut upstream_request = client().request(parts.method.clone(), &url);
    for (name, value) in &parts.headers {
        if !dropped(name, &DROPPED_REQUEST_HEADERS) && !name.as_str().starts_with("x-forwarded-") {
            upstream_request = upstream_request.header(name, value);
        }
    }
    if let Some(cookies) = app_cookies(&parts.headers) {
        upstream_request = upstream_request.header(header::COOKIE, cookies);
    }
    upstream_request = upstream_request.header("x-forwarded-prefix", app.served_at());
    let has_body = parts.headers.contains_key(header::TRANSFER_ENCODING)
        || parts
            .headers
            .get(header::CONTENT_LENGTH)
            .is_some_and(|length| length != "0");
    if has_body {
        upstream_request =
            upstream_request.body(reqwest::Body::wrap_stream(body.into_data_stream()));
    }
    let upstream_response = match upstream_request.send().await {
        Ok(response) => response,
        Err(e) => return bad_gateway(&app, &e.to_string()),
    };
    let status = upstream_response.status();
    let headers = upstream_response.headers().clone();
    let body = if parts.method == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(upstream_response.bytes_stream())
    };
    let mut response = Response::new(body);
    *response.status_mut() = status;
    for (name, value) in &headers {
        let composers =
            name == header::SET_COOKIE && value.to_str().map_or(true, is_composer_cookie);
        let theirs = composers || dropped(name, &DROPPED_RESPONSE_HEADERS);
        if !theirs {
            response.headers_mut().append(name, value.clone());
        }
    }
    if let Some(location) = headers.get(header::LOCATION)
        && let Some(path) = location.to_str().ok().and_then(|l| app.local_path(l))
        && let Ok(value) = HeaderValue::from_str(&path)
    {
        response.headers_mut().insert(header::LOCATION, value);
    }
    // Keeps a front proxy from holding an event stream back.
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"))
    {
        response
            .headers_mut()
            .insert("x-accel-buffering", HeaderValue::from_static("no"));
    }
    response
}

fn dropped(name: &HeaderName, list: &[&str]) -> bool {
    list.contains(&name.as_str())
}

/// A WebSocket: the handshake goes to the app (with the client's subprotocols and its cookies
/// without composer's, no `Origin`), then every text and binary message is relayed both ways.
async fn socket(app: AppUpstream, url: String, request: Request) -> Response {
    let protocols = request
        .headers()
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .cloned();
    let cookies = app_cookies(request.headers());
    let upgrade = match WebSocketUpgrade::from_request(request, &()).await {
        Ok(upgrade) => upgrade,
        Err(rejected) => return rejected.into_response(),
    };
    let mut handshake = match url.into_client_request() {
        Ok(handshake) => handshake,
        Err(e) => return bad_gateway(&app, &e.to_string()),
    };
    if let Some(protocols) = protocols {
        handshake
            .headers_mut()
            .insert(header::SEC_WEBSOCKET_PROTOCOL, protocols);
    }
    if let Some(cookies) = cookies {
        handshake.headers_mut().insert(header::COOKIE, cookies);
    }
    if let Ok(prefix) = HeaderValue::from_str(&app.served_at()) {
        handshake.headers_mut().insert("x-forwarded-prefix", prefix);
    }
    let (upstream, answer) = match tokio_tungstenite::connect_async(handshake).await {
        Ok(connected) => connected,
        Err(e) => return bad_gateway(&app, &e.to_string()),
    };
    let chosen = answer
        .headers()
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let upgrade = match chosen {
        Some(chosen) => upgrade.protocols([chosen]),
        None => upgrade,
    };
    upgrade.on_upgrade(move |client| relay(client, upstream))
}

type Upstream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Text and binary messages both ways, in order; either side closing (or failing) ends both,
/// the close's code and reason passed on. Pings are answered by each side's library.
async fn relay(client: WebSocket, upstream: Upstream) {
    let (mut to_client, mut from_client) = client.split();
    let (mut to_upstream, mut from_upstream) = upstream.split();
    let mut client_close = None;
    let mut upstream_close = None;

    let up = async {
        // Keeps the library's queued pongs going out while the client says nothing.
        let mut keepalive = tokio::time::interval(Duration::from_secs(15));
        loop {
            tokio::select! {
                message = from_client.next() => {
                    let message = match message {
                        Some(Ok(Message::Text(text))) => {
                            tungstenite::Message::Text(text.as_str().into())
                        }
                        Some(Ok(Message::Binary(data))) => tungstenite::Message::Binary(data),
                        Some(Ok(Message::Close(frame))) => {
                            client_close = frame.map(|f| tungstenite::protocol::CloseFrame {
                                code: f.code.into(),
                                reason: f.reason.as_str().into(),
                            });
                            return;
                        }
                        Some(Ok(_)) => continue,
                        Some(Err(_)) | None => return,
                    };
                    if to_upstream.send(message).await.is_err() {
                        return;
                    }
                }
                _ = keepalive.tick() => {
                    if to_upstream.flush().await.is_err() {
                        return;
                    }
                }
            }
        }
    };
    let down = async {
        while let Some(Ok(message)) = from_upstream.next().await {
            let message = match message {
                tungstenite::Message::Text(text) => Message::Text(text.as_str().into()),
                tungstenite::Message::Binary(data) => Message::Binary(data),
                tungstenite::Message::Close(frame) => {
                    upstream_close = frame.map(|f| CloseFrame {
                        code: f.code.into(),
                        reason: f.reason.as_str().into(),
                    });
                    return;
                }
                _ => continue,
            };
            if to_client.send(message).await.is_err() {
                return;
            }
        }
    };
    tokio::select! {
        _ = up => {}
        _ = down => {}
    }
    let _ = to_client.send(Message::Close(upstream_close)).await;
    let _ = to_upstream
        .send(tungstenite::Message::Close(client_close))
        .await;
    let _ = to_upstream.close().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composers_parameters_and_cookies_stay_here() {
        assert_eq!(
            upstream_query(Some("a=1&token=x&%74oken=y&once=z&b")),
            "a=1&b"
        );
        assert_eq!(upstream_query(None), "");
        let mut headers = HeaderMap::new();
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("composer_token_7420=s; theme=dark"),
        );
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("composer_token=t;app=1"),
        );
        assert_eq!(app_cookies(&headers).unwrap(), "theme=dark; app=1");
        let mut only_ours = HeaderMap::new();
        only_ours.insert(
            header::COOKIE,
            HeaderValue::from_static("composer_token_1=s"),
        );
        assert_eq!(app_cookies(&only_ours), None);
        assert!(is_composer_cookie("composer_token_7420=x; Path=/"));
        assert!(!is_composer_cookie("session=composer_token"));
    }
}
