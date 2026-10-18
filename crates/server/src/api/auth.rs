use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::server::Server;

const COOKIE: &str = "composer_token";

/// Requires the token: `Authorization: Bearer <t>`, `?token=<t>`, or the cookie that a
/// `?token=` request sets (so an app opened with the token can load its scripts and call
/// the API). Browser requests must also come from our own origin.
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
        .is_some_and(|t| t == server.token);
    let query = request
        .uri()
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .any(|(k, v)| k == "token" && v == server.token);
    let cookie = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .any(|(k, v)| k == COOKIE && v == server.token);
    if !(bearer || query || cookie) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong token").into_response();
    }

    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        let host = headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let same_origin = origin
            .split_once("://")
            .is_some_and(|(_, rest)| rest == host);
        if !same_origin {
            return (StatusCode::FORBIDDEN, "cross-origin request").into_response();
        }
    }

    let mut response = next.run(request).await;
    if query && !cookie {
        let value = format!(
            "{COOKIE}={}; Path=/; HttpOnly; SameSite=Strict",
            server.token
        );
        if let Ok(value) = HeaderValue::from_str(&value) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}
