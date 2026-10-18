//! Serving built micro apps (`<site>/ui/dist` at `/<site>/`) and raw site files.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use composer_protocol::http::PutPageResponse;

use crate::api::ApiError;
use crate::server::Server;

/// `/<site>` → `/<site>/` (relative asset paths need the slash), keeping the query.
pub async fn app_redirect(Path(site): Path<String>, uri: Uri) -> Redirect {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    Redirect::permanent(&format!("/{site}/{query}"))
}

pub async fn app_index(State(server): State<Arc<Server>>, Path(site): Path<String>) -> Response {
    serve_app(&server, &site, "")
}

pub async fn app_file(
    State(server): State<Arc<Server>>,
    Path((site, path)): Path<(String, String)>,
) -> Response {
    serve_app(&server, &site, &path)
}

/// `GET /api/files/<site>/<path>`: any file of a site, with its content type.
pub async fn get_file(
    State(server): State<Arc<Server>>,
    Path((site, path)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let url = SiteUrl::new(&site, &path)?;
    let file = server.workspace.root().join(url.site()).join(url.path());
    let bytes = std::fs::read(&file)
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, format!("{url} not found")))?;
    Ok(([(header::CONTENT_TYPE, content_type(url.path()))], bytes).into_response())
}

/// `PUT /api/files/<site>/<path>`: writes the request body as the file.
pub async fn put_file(
    State(server): State<Arc<Server>>,
    Path((site, path)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<PutPageResponse>, ApiError> {
    let url = SiteUrl::new(&site, &path)?;
    if url.path().is_empty() || url.path().ends_with('/') {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "a file path is required".into(),
        ));
    }
    let (written, _) = server.write_file(&url, &body, ChangedBy::App)?;
    Ok(Json(PutPageResponse {
        url: written.to_string(),
        hash: crate::content_hash(&body),
    }))
}

fn serve_app(server: &Server, site: &str, path: &str) -> Response {
    let Ok(url) = SiteUrl::new(site, path) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let dist = server
        .workspace
        .root()
        .join(url.site())
        .join("ui")
        .join("dist");
    if !dist.join("index.html").is_file() {
        return (
            StatusCode::NOT_FOUND,
            Html(format!(
                "<!doctype html><meta charset=utf-8><body style=\"background:#0c0d0f;color:#868c96;font:14px monospace;padding:40px\">\
                 site://{site} has no built app. Ask the agent to run its <code>ui-build</code> action \
                 (or create one with <code>new_app</code>).</body>"
            )),
        )
            .into_response();
    }
    // `url.path()` is normalized and cannot leave the site.
    let requested = url.path();
    let mut file = dist.join(requested);
    let has_extension = requested
        .rsplit('/')
        .next()
        .is_some_and(|f| f.contains('.'));
    if requested.is_empty() || !file.is_file() && !has_extension {
        file = dist.join("index.html"); // client-side routes fall back to the app
    }
    match std::fs::read(&file) {
        Ok(bytes) => {
            let name = file.to_string_lossy();
            let cache = if name.ends_with("index.html") {
                "no-cache"
            } else {
                "max-age=31536000, immutable"
            };
            (
                [
                    (header::CONTENT_TYPE, content_type(&name)),
                    (header::CACHE_CONTROL, cache),
                ],
                bytes,
            )
                .into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn content_type(path: &str) -> &'static str {
    let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match extension.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "md" => "text/markdown; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}
