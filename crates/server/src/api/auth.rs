use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::server::Server;

/// Requires the token (`Authorization: Bearer <t>` or `?token=<t>`), and that
/// browser requests come from our own origin (blocks other sites reaching localhost).
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
    if !(bearer || query) {
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
    next.run(request).await
}
