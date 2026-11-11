use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::api::WebSessions;
use crate::server::Server;

/// The auth cookie's name; served alone (no `/w/<name>` in front), the listener's port is added
/// to it ([`cookie_name`]).
pub const COOKIE: &str = "composer_token";

/// The name of this workspace's auth cookie. Under `/w/<name>` the cookie's `Path` keeps
/// workspaces apart. Served alone its `Path` is `/`, and cookies are not scoped by port: the
/// listener's port in the name (`composer_token_7420`) keeps servers on other ports of the same
/// host from overwriting or receiving it.
pub fn cookie_name(server: &Server) -> String {
    match (server.route_base(), server.listen_addr()) {
        ("", Some(addr)) => format!("{COOKIE}_{}", addr.port()),
        _ => COOKIE.to_string(),
    }
}

/// Requires a credential of this workspace (its own token or a paired device's, or a browser
/// session one of them opened); the kind that authenticated goes on in the request as a
/// [`crate::api::Credential`]: `Authorization: Bearer <t>`, `?token=<t>`, `?once=<code>`, or the cookie.
/// Browser requests must also come from our own origin.
///
/// The cookie never holds a token. A request that presents `?token=` or `?once=` and no valid
/// cookie gets a new browser session ([`crate::api::WebSessions`]) as its cookie: a random secret
/// the server keeps in memory, of the same kind as the token (or as whoever asked for the
/// one-time code), gone when that device is revoked or after its idle time.
///
/// Without a credential the answer is 404, the same as for a workspace that doesn't exist: a
/// token for one workspace can't even learn that another exists.
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
    // A client app's one-time code (ticket 0168): used once, it opens a session of whoever asked
    // for it, if that one still counts.
    let once = params()
        .filter(|(k, _)| *k == "once")
        .find_map(|(_, v)| server.once_codes.take(v))
        .filter(|minter| server.credential_alive(minter));
    let name = cookie_name(&server);
    let cookie = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .filter(|(k, _)| *k == name)
        .find_map(|(_, v)| server.credential(v));
    let from_url = query_token.is_some() || once.is_some();
    let credential = bearer
        .clone()
        .or(query_token.as_ref().map(|(c, _)| c.clone()))
        .or(once.clone())
        .or(cookie.clone());
    let Some(credential) = credential else {
        return (
            StatusCode::NOT_FOUND,
            "not found (or not with this token: each workspace has its own)",
        )
            .into_response();
    };

    if let Some(refused) = origin_refused(headers, Audience::Workspace) {
        return refused;
    }
    // A browser holding the workspace's cookie sends it from an app's page too: an app page
    // calls its own API with its own token instead (ticket 0163). (An app page is sandboxed, so
    // its requests say `Origin: null` and are refused above; this is the second fence.)
    if cookie.is_some()
        && bearer.is_none()
        && !from_url
        && from_app_page(headers, server.route_base())
    {
        return (
            StatusCode::FORBIDDEN,
            "an app page calls its own API (<base>/a/<app token>/api/...), not the workspace's",
        )
            .into_response();
    }

    // The cookie for a browser that came with `?token=` or `?once=` and has none that works: a
    // session of that credential's kind, never the token. A session presented as `?token=` stays
    // the one it is. A socket's handshake gets none: its client keeps what it connected with.
    let upgrade = headers.contains_key(header::UPGRADE);
    let session = match (cookie.is_some() || upgrade, query_token, once) {
        (true, _, _) | (false, None, None) => None,
        (false, Some((_, token)), _) if token.starts_with(WebSessions::PREFIX) => Some(token),
        (false, Some((minter, _)), _) | (false, None, Some(minter)) => {
            Some(server.web_sessions.mint(minter))
        }
    };
    let secure = is_https(headers);
    request.extensions_mut().insert(credential);
    let mut response = next.run(request).await;
    if let Some(session) = session {
        set_cookie(
            &mut response,
            &name,
            &session,
            secure,
            None,
            server.route_base(),
        );
    }
    response
}

/// The request comes from an app page (its `Referer` is under `<base>/a/`, or the old `/p/` and
/// `/h/`).
fn from_app_page(headers: &HeaderMap, route_base: &str) -> bool {
    let prefixes = ["/a/", "/p/", "/h/"].map(|p| format!("{route_base}{p}"));
    headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(|r| r.split_once("://").map(|(_, rest)| rest))
        .and_then(|rest| rest.find('/').map(|at| &rest[at..]))
        .is_some_and(|path| prefixes.iter().any(|p| path.starts_with(p.as_str())))
}

/// Whom a route serves, for the Origin rule ([`origin_refused`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// The workspace's routes (its API, `/ws`, the viewer proxy, pairing): its own pages only.
    Workspace,
    /// An app's own routes (`<base>/a/<app token>/...`): its page is sandboxed (an opaque origin,
    /// `Origin: null`), and its token is in the path, not in a cookie.
    App,
}

/// The one Origin rule. A browser request (it has an `Origin`) is accepted when it comes from our
/// own origin; `Origin: null` (a sandboxed app page, and anything else without an origin of its
/// own) only on an app's routes, which no cookie authenticates. Everything else: 403. A request
/// without `Origin` (a client that is no browser, a plain navigation) is the credential's matter.
pub fn origin_refused(headers: &HeaderMap, audience: Audience) -> Option<Response> {
    let origin = headers.get(header::ORIGIN)?;
    let allowed = match origin.to_str() {
        Ok("null") => audience == Audience::App,
        Ok(origin) => same_origin(origin, headers),
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
/// `max_age` in seconds makes it outlive the browser session (paired devices). Scoped to the
/// workspace's routes (`route_base`), so each workspace keeps its own cookie in one browser.
pub fn set_cookie(
    response: &mut Response,
    name: &str,
    token: &str,
    secure: bool,
    max_age: Option<u64>,
    route_base: &str,
) {
    let mut value = format!("{name}={token}; Path={route_base}/; HttpOnly; SameSite=Strict");
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
