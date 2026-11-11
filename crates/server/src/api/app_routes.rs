//! Serving built micro apps (`<site>/ui/dist`) and raw site files. At `<base>/<site>/` only
//! client apps are served (approved builds only); every other app is served sandboxed under its
//! own token (`app_api.rs`).

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

/// `<base>/<site>` → `<base>/<site>/` (relative asset paths need the slash), keeping the query.
/// The target is relative (`<site>/`), so it lands under whichever workspace served it.
pub async fn app_redirect(Path(site): Path<String>, uri: Uri) -> Redirect {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    Redirect::permanent(&format!("{site}/{query}"))
}

pub async fn app_index(State(server): State<Arc<Server>>, Path(site): Path<String>) -> Response {
    match client_only(&server, &site) {
        Some(refused) => refused,
        None => serve_client(&server, &site, ""),
    }
}

pub async fn app_file(
    State(server): State<Arc<Server>>,
    Path((site, path)): Path<(String, String)>,
) -> Response {
    match client_only(&server, &site) {
        Some(refused) => refused,
        None => serve_client(&server, &site, &path),
    }
}

/// A client app's file (`<base>/<site>/`, unsandboxed, with the workspace's credentials): only
/// the build the user approved ([`crate::apps::ClientApprovals`]). The bytes checked are the
/// bytes sent; its page also checks the whole build, so a changed one says so there.
fn serve_client(server: &Server, site: &str, path: &str) -> Response {
    let Ok(url) = SiteUrl::new(site, path) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let dist = server.web.root().join(url.site()).join("ui").join("dist");
    if !dist.join("index.html").is_file() {
        // Not built (or no app): what `serve_app` says about it.
        return serve_app(server, site, path);
    }
    let file = pick(&dist, url.path());
    if let Err(e) = server.web.check(&file) {
        return ApiError::from(e).into_response();
    }
    let Ok(bytes) = std::fs::read(&file) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let relative = file
        .strip_prefix(&dist)
        .map(crate::apps::relative_path)
        .unwrap_or_default();
    let page = file.extension().is_some_and(|x| x == "html");
    let approved = server.client_apps.allows_file(site, &relative, &bytes)
        && (!page || server.client_apps.allows(site, &dist));
    if !approved {
        return unapproved(server, site);
    }
    send_bytes(&file, bytes)
}

/// The page a client app shows when its build isn't the one the user approved.
fn unapproved(server: &Server, site: &str) -> Response {
    let workspace = server.home.name().as_str();
    let flag = match workspace {
        "default" => String::new(),
        name => format!(" --workspace {name}"),
    };
    let message = format!(
        "site://{site} is a client app: it runs with this workspace's full access, so it is served \
         only as you approved it, and its build (<code>{site}/ui/dist</code>) is new or changed \
         since. Look at what changed (an agent can write it), then approve it as it is now: \
         <code>composer app approve {site}{flag}</code>, and reload."
    );
    (
        StatusCode::FORBIDDEN,
        Html(format!(
            "<!doctype html><meta charset=utf-8><body style=\"background:#0c0d0f;color:#868c96;font:14px monospace;padding:40px\">{message}</body>"
        )),
    )
        .into_response()
}

/// `<base>/<site>/` (the workspace's token) serves client apps only (ticket 0168); any other app
/// is opened with a token of its own: the refusal says how. It fails closed: a site whose
/// manifest cannot be read (a broken `app:` block, no such site) is refused too, never served
/// under the workspace's credentials. `None` (go on to [`serve_app`]) only for a client app with
/// a manifest that parses, and for a site that is no app at all, where there is nothing to serve
/// and `serve_app` says so.
fn client_only(server: &Server, site: &str) -> Option<Response> {
    let message = match crate::apps::site_manifest(server, site) {
        Ok(None) if !has_ui(server, site) => return None,
        Ok(Some(_)) if crate::apps::is_client(server, site) => return None,
        Ok(_) => format!(
            "site://{site}'s app opens with a token of its own: <code>POST /api/apps/{site}/open</code> \
             (Home) or <code>POST /api/projects/&lt;project&gt;/apps/{site}/open</code>, then load the \
             URL it gives (the TUI's <code>a</code> does this)."
        ),
        Err(why) => format!(
            "site://{site}'s app is not served: {}",
            why.replace('&', "&amp;").replace('<', "&lt;")
        ),
    };
    Some(
        (
            StatusCode::NOT_FOUND,
            Html(format!(
                "<!doctype html><meta charset=utf-8><body style=\"background:#0c0d0f;color:#868c96;font:14px monospace;padding:40px\">{message}</body>"
            )),
        )
            .into_response(),
    )
}

/// Whether anything exists at `site`'s `ui` (a folder, or whatever else is there): asked of the
/// file system itself, so "no app" never rests on the manifest alone.
fn has_ui(server: &Server, site: &str) -> bool {
    SiteUrl::new(site, "").is_ok_and(|url| {
        server
            .web
            .root()
            .join(url.site())
            .join("ui")
            .symlink_metadata()
            .is_ok()
    })
}

/// `GET /api/files/<site>/<path>`: any file of a site, with its content type.
pub async fn get_file(
    State(server): State<Arc<Server>>,
    Path((site, path)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let url = SiteUrl::new(&site, &path)?;
    let file = server.web.root().join(url.site()).join(url.path());
    // A refused mount gives its own error; the route must not read through it.
    server.web.check(&file)?;
    let bytes = std::fs::read(&file)
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, format!("{url} not found")))?;
    // A site's files are agent-writable: an HTML one opened as a page runs sandboxed, never with
    // the workspace's cookie.
    Ok((
        [
            (header::CONTENT_TYPE, content_type(url.path())),
            (header::CONTENT_SECURITY_POLICY, crate::api::SANDBOX),
        ],
        bytes,
    )
        .into_response())
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

pub(crate) fn serve_app(server: &Server, site: &str, path: &str) -> Response {
    let Ok(url) = SiteUrl::new(site, path) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let dist = server.web.root().join(url.site()).join("ui").join("dist");
    if !dist.join("index.html").is_file() {
        let site = url.site();
        let message = if dist.parent().is_some_and(|ui| ui.is_dir()) {
            format!(
                "site://{site} has an app, but it is not built: <code>{site}/ui/dist/index.html</code> \
                 is missing. Ask the agent to run the site's <code>ui-build</code> action."
            )
        } else {
            format!(
                "site://{site} has no app: there is no <code>{site}/ui/</code> folder. \
                 The agent can create one with <code>new_app</code>."
            )
        };
        return (
            StatusCode::NOT_FOUND,
            Html(format!(
                "<!doctype html><meta charset=utf-8><body style=\"background:#0c0d0f;color:#868c96;font:14px monospace;padding:40px\">{message}</body>"
            )),
        )
            .into_response();
    }
    // `url.path()` is normalized and cannot leave the site; a refused mount still answers.
    let file = pick(&dist, url.path());
    if let Err(e) = server.web.check(&file) {
        return ApiError::from(e).into_response();
    }
    send_file(&file)
}

/// A project-local app's `ui/dist` (`dir` is its folder): like a site's, and no symlink in it may
/// lead out of it.
pub(crate) fn serve_local(name: &str, dir: &std::path::Path, path: &str) -> Response {
    let Ok(url) = SiteUrl::new(name, path) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let dist = dir.join("ui").join("dist");
    let file = pick(&dist, url.path());
    let inside = std::fs::canonicalize(&dist)
        .ok()
        .zip(std::fs::canonicalize(&file).ok())
        .is_some_and(|(dist, file)| file.starts_with(dist));
    if !inside {
        return (
            StatusCode::NOT_FOUND,
            format!("the local app `{name}` isn't built: no ui/dist/index.html in it"),
        )
            .into_response();
    }
    send_file(&file)
}

/// The file of `dist` for `requested` (normalized); client-side routes fall back to the app.
fn pick(dist: &std::path::Path, requested: &str) -> std::path::PathBuf {
    let file = dist.join(requested);
    let has_extension = requested
        .rsplit('/')
        .next()
        .is_some_and(|f| f.contains('.'));
    if requested.is_empty() || !file.is_file() && !has_extension {
        return dist.join("index.html");
    }
    file
}

fn send_file(file: &std::path::Path) -> Response {
    match std::fs::read(file) {
        Ok(bytes) => send_bytes(file, bytes),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn send_bytes(file: &std::path::Path, bytes: Vec<u8>) -> Response {
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
