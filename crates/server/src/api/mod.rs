//! The WS/HTTP API. Every route requires the token.

mod api_error;
mod auth;
mod connection;
mod event_scope;
mod glob;
mod http_routes;
mod ws_routes;

use std::sync::Arc;

use axum::Router;
use axum::middleware;
use axum::routing::{get, post};

pub use api_error::ApiError;
pub use connection::Connection;

use crate::server::Server;

pub fn router(server: Arc<Server>) -> Router {
    Router::new()
        .route("/ws", get(ws_routes::upgrade))
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
            "/api/sessions/{id}/messages",
            post(http_routes::post_message),
        )
        .layer(middleware::from_fn_with_state(
            server.clone(),
            auth::require_token,
        ))
        .with_state(server)
}
