//! The WS/HTTP API. Every route requires a token, except pairing.

mod api_error;
mod app_routes;
mod auth;
mod connection;
mod event_scope;
mod glob;
mod http_routes;
mod pair_routes;
mod resequence;
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
        .route("/api/info", get(http_routes::info))
        .route("/api/sites", get(http_routes::sites))
        .route("/api/agents", get(http_routes::agents))
        .route("/api/projects", get(http_routes::projects))
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
        .route(
            "/api/sessions/{id}/project/diff",
            get(http_routes::project_diff),
        )
        .route(
            "/api/files/{site}/{*path}",
            get(app_routes::get_file).put(app_routes::put_file),
        )
        // Built micro apps; routes with a fixed first segment (/api, /ws) take precedence.
        .route("/{site}", get(app_routes::app_redirect))
        .route("/{site}/", get(app_routes::app_index))
        .route("/{site}/{*path}", get(app_routes::app_file))
        .layer(middleware::from_fn_with_state(
            server.clone(),
            auth::require_token,
        ))
        // Pairing needs no token: the one-time code is the credential.
        .merge(
            Router::new()
                .route(
                    "/pair",
                    get(pair_routes::pair_page).post(pair_routes::pair_submit),
                )
                .route("/api/pair", post(pair_routes::pair_api)),
        )
        .with_state(server)
}
