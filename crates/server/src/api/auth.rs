use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::server::Server;

pub const COOKIE: &str = "composer_token";

/// Requires a token of this workspace (its own or a device paired to it): `Authorization:
/// Bearer <t>`, `?token=<t>`, or the cookie that a `?token=` request (or pairing) sets. Browser
/// requests must also come from our own origin.
///
/// Without one the answer is 404, the same as for a workspace that doesn't exist: a token for
/// one workspace can't even learn that another exists.
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
    // A client app's one-time code (ticket 0168): used once, it becomes the cookie.
    let once = request
        .uri()
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .any(|(k, v)| k == "once" && server.once_codes.take(v));
    let query_token = query_token.or_else(|| once.then(|| server.token.clone()));
    let cookie = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .any(|(k, v)| k == COOKIE && server.accepts_token(v));
    if !(bearer || query_token.is_some() || cookie) {
        return (
            StatusCode::NOT_FOUND,
            "not found (or not with this token: each workspace has its own)",
        )
            .into_response();
    }

    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
        && !same_origin(origin, headers)
    {
        return (StatusCode::FORBIDDEN, "cross-origin request").into_response();
    }
    // A browser holding the workspace's cookie sends it from an app's page too: an app page
    // calls its own API with its own token instead (ticket 0163).
    if cookie && !bearer && query_token.is_none() && from_app_page(headers, server.route_base()) {
        return (
            StatusCode::FORBIDDEN,
            "an app page calls its own API (<base>/p/<project>/<app>/api/...) with its app token, \
             not the workspace's",
        )
            .into_response();
    }

    let secure = is_https(headers);
    let mut response = next.run(request).await;
    if let Some(token) = query_token.filter(|_| !cookie) {
        set_cookie(&mut response, &token, secure, None, server.route_base());
    }
    response
}

/// The request comes from an app page with a token of its own (its `Referer` is under
/// `<base>/p/` or `<base>/h/`).
fn from_app_page(headers: &HeaderMap, route_base: &str) -> bool {
    let (project_apps, home_apps) = (format!("{route_base}/p/"), format!("{route_base}/h/"));
    headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(|r| r.split_once("://").map(|(_, rest)| rest))
        .and_then(|rest| rest.find('/').map(|at| &rest[at..]))
        .is_some_and(|path| path.starts_with(&project_apps) || path.starts_with(&home_apps))
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
/// makes it outlive the browser session (paired devices). Scoped to the workspace's routes
/// (`route_base`), so each workspace keeps its own cookie in one browser.
pub fn set_cookie(
    response: &mut Response,
    token: &str,
    secure: bool,
    max_age: Option<u64>,
    route_base: &str,
) {
    let mut value = format!("{COOKIE}={token}; Path={route_base}/; HttpOnly; SameSite=Strict");
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
