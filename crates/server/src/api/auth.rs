use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::api::WebSessions;
use crate::server::Server;

/// The auth cookie's name; the listener's port is added to it ([`cookie_name`]).
pub const COOKIE: &str = "composer_token";

/// The name of the auth cookie. Its `Path` is `/`, and cookies are not scoped by port: the
/// listener's port in the name (`composer_token_7420`) keeps servers on other ports of the same
/// host from overwriting or receiving it.
pub fn cookie_name(server: &Server) -> String {
    match server.listen_addr() {
        Some(addr) => format!("{COOKIE}_{}", addr.port()),
        None => COOKIE.to_string(),
    }
}

/// Requires a credential (the local token or a paired device's, or a browser session one of them
/// opened); the kind that authenticated goes on in the request as a [`crate::api::Credential`]:
/// `Authorization: Bearer <t>`, `?token=<t>`, or the cookie. Browser requests must also come from
/// our own origin.
///
/// The cookie never holds a token. A request that presents `?token=` and no valid cookie gets a
/// new browser session ([`crate::api::WebSessions`]) as its cookie: a random secret the server
/// keeps in memory, of the same kind as the token, gone when that device is revoked or after its
/// idle time.
///
/// Without a credential the answer is 404.
pub async fn require_token(
    State(server): State<Arc<Server>>,
    mut request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|t| server.credential(t));
    let query = request.uri().query().unwrap_or("");
    let params = || query.split('&').filter_map(|pair| pair.split_once('='));
    let query_token = params()
        .filter(|(k, _)| *k == "token")
        .find_map(|(_, v)| server.credential(v).map(|c| (c, v.to_string())));
    let name = cookie_name(&server);
    let cookie = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .filter(|(k, _)| *k == name)
        .find_map(|(_, v)| server.credential(v));
    let credential = bearer
        .clone()
        .or(query_token.as_ref().map(|(c, _)| c.clone()))
        .or(cookie.clone());
    let Some(credential) = credential else {
        return (StatusCode::NOT_FOUND, "not found (or not with this token)").into_response();
    };

    if let Some(refused) = origin_refused(headers) {
        return refused;
    }
    // The cookie for a browser that came with `?token=` and has none that works: a session of
    // that credential's kind, never the token. A session presented as `?token=` stays the one it
    // is. A socket's handshake gets none: its client keeps what it connected with.
    let upgrade = headers.contains_key(header::UPGRADE);
    let session = match (cookie.is_some() || upgrade, query_token) {
        (true, _) | (false, None) => None,
        (false, Some((_, token))) if token.starts_with(WebSessions::PREFIX) => Some(token),
        (false, Some((minter, _))) => Some(server.web_sessions.mint(minter)),
    };
    let secure = is_https(headers);
    request.extensions_mut().insert(credential);
    let mut response = next.run(request).await;
    if let Some(session) = session {
        set_cookie(&mut response, &name, &session, secure, None);
    }
    response
}

/// The one Origin rule. A browser request (it has an `Origin`) is accepted when it comes from our
/// own origin; anything else (`Origin: null` included): 403. A request without `Origin` (a client
/// that is no browser, a plain navigation) is the credential's matter.
pub fn origin_refused(headers: &HeaderMap) -> Option<Response> {
    let origin = headers.get(header::ORIGIN)?;
    let allowed = match origin.to_str() {
        Ok(origin) => origin != "null" && same_origin(origin, headers),
        Err(_) => false,
    };
    (!allowed).then(|| (StatusCode::FORBIDDEN, "cross-origin request").into_response())
}

/// The browser's origin is our host, directly or as `tailscale serve` forwards it.
fn same_origin(origin: &str, headers: &HeaderMap) -> bool {
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

/// The auth cookie `name` ([`cookie_name`]): HttpOnly, SameSite=Strict, `Secure` over HTTPS.
/// `max_age` in seconds makes it outlive the browser session (paired devices).
pub fn set_cookie(
    response: &mut Response,
    name: &str,
    token: &str,
    secure: bool,
    max_age: Option<u64>,
) {
    let mut value = format!("{name}={token}; Path=/; HttpOnly; SameSite=Strict");
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
