//! The WS/HTTP API. Every route requires a token, except pairing.

mod api_error;
mod attachment_routes;
mod auth;
mod connection;
mod credential;
mod event_scope;
mod frame_queue;
pub(crate) mod glob;
mod hook_routes;
mod http_routes;
mod pair_routes;
mod view_routes;
pub(crate) mod viewer_proxy;
mod web_sessions;
mod ws_routes;

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{get, post};

pub use api_error::ApiError;
pub use connection::Connection;
pub use credential::Credential;
pub use web_sessions::WebSessions;

use crate::server::Server;

pub fn router(server: Arc<Server>) -> Router {
    Router::new()
        .route("/ws", get(ws_routes::upgrade))
        .route("/api/info", get(http_routes::info))
        .route("/api/web-session", post(http_routes::web_session))
        .route("/api/sites", get(http_routes::sites))
        .route("/api/search", get(http_routes::search))
        .route(
            "/api/pages/{site}",
            get(http_routes::get_site_index).put(http_routes::put_site_index),
        )
        .route(
            "/api/pages/{site}/{*path}",
            get(http_routes::get_page).put(http_routes::put_page),
        )
        .route(
            "/api/sessions/{id}/attachments",
            post(attachment_routes::upload).layer(DefaultBodyLimit::max(
                ((server.config().attachments.max_mb * attachment_routes::FILES_PER_REQUEST + 1)
                    * 1024
                    * 1024) as usize,
            )),
        )
        .route(
            "/api/sessions/{id}/attachments/{file}",
            get(attachment_routes::get),
        )
        .route(
            "/api/sessions/{id}/project/diff",
            get(http_routes::project_diff),
        )
        // wayai's live view, proxied view-only. Its fixed paths come before the page views'
        // `/view/{site}`, which they would otherwise match.
        .merge(viewer_proxy::router())
        // Read-only HTML views of pages and changes (the desktop's tabs).
        .route("/view/changes/{site}", get(view_routes::changes))
        .route("/view/{site}", get(view_routes::site_index))
        .route("/view/{site}/{*path}", get(view_routes::page))
        .layer(middleware::from_fn_with_state(
            server.clone(),
            auth::require_token,
        ))
        // Pairing needs no token: the one-time code is the credential. A webhook has its own.
        .merge(
            Router::new()
                .route(
                    "/pair",
                    get(pair_routes::pair_page).post(pair_routes::pair_submit),
                )
                .route("/api/pair", post(pair_routes::pair_api))
                .route("/api/hooks/{name}", post(hook_routes::call)),
        )
        .with_state(server)
}
