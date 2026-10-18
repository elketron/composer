use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::server::Server;

pub const COOKIE: &str = "composer_token";

/// Requires a token (the local one or a paired device's): `Authorization: Bearer <t>`,
/// `?token=<t>`, or the cookie that a `?token=` request (or pairing) sets. Browser
/// requests must also come from our own origin.
pub async fn require_token(
    State(server): State<Arc<Server>>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| server.accepts_token(t));
    let query_token = request
        .uri()
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, v)| *k == "token" && server.accepts_token(v))
        .map(|(_, v)| v.to_string());
    let cookie = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .any(|(k, v)| k == COOKIE && server.accepts_token(v));
    if !(bearer || query_token.is_some() || cookie) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong token").into_response();
    }

    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
        && !same_origin(origin, headers)
    {
        return (StatusCode::FORBIDDEN, "cross-origin request").into_response();
    }

    let secure = is_https(headers);
    let mut response = next.run(request).await;
    if let Some(token) = query_token.filter(|_| !cookie) {
        set_cookie(&mut response, &token, secure, None);
    }
    response
}

/// The browser's origin is our host, directly or as `tailscale serve` forwards it.
pub fn same_origin(origin: &str, headers: &HeaderMap) -> bool {
    let Some((_, origin_host)) = origin.split_once("://") else {
        return false;
    };
    [header::HOST.as_str(), "x-forwarded-host"]
        .iter()
        .filter_map(|name| headers.get(*name).and_then(|v| v.to_str().ok()))
        .any(|host| host == origin_host)
}

/// Behind `tailscale serve` the browser speaks HTTPS even though we see HTTP.
pub fn is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        == Some("https")
}

/// The auth cookie: HttpOnly, SameSite=Strict, `Secure` over HTTPS. `max_age` in seconds
/// makes it outlive the browser session (paired devices).
pub fn set_cookie(response: &mut Response, token: &str, secure: bool, max_age: Option<u64>) {
    let mut value = format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict");
    if secure {
        value.push_str("; Secure");
    }
    if let Some(age) = max_age {
        value.push_str(&format!("; Max-Age={age}"));
    }
    if let Ok(value) = HeaderValue::from_str(&value) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
}
