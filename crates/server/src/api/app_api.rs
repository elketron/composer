//! A project app's own routes (ticket 0163): `<base>/p/<project>/<app>/` serves the app, and
//! `<base>/p/<project>/<app>/api/...` is the API as its token allows (decision 2026-11-07): read any
//! page of the workspace; write only its own site and its project; run only its own site's
//! actions, on its project. These routes sit outside the workspace token's: an app page never
//! needs (and never gets) the workspace token.

use std::collections::HashMap;
use std::path::{Component, Path as FsPath, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, RawPathParams, Request, State, WebSocketUpgrade};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use composer_core::SiteUrl;
use composer_protocol::http::{
    ActionSummary, AppMessage, PageResponse, ProjectInfo, PutPageRequest, PutPageResponse,
    RunActionRequest, RunActionResponse, SearchHit, SiteSummary,
};

use crate::api::auth::{is_https, same_origin};
use crate::api::{ApiError, action_routes, app_routes, http_routes, ws_routes};
use crate::apps::AppGrant;
use crate::server::Server;

/// The cookie an app page's token rides in, scoped to the app's own path.
pub const APP_COOKIE: &str = "composer_app";

/// The routes, to merge into a workspace's router (outside its token layer).
pub fn router(server: Arc<Server>) -> Router<Arc<Server>> {
    let base = "/p/{project}/{app}";
    Router::new()
        .route(base, get(redirect))
        .route(&format!("{base}/"), get(index))
        .route(&format!("{base}/{{*path}}"), get(file))
        .route(&format!("{base}/ws"), get(ws))
        .route(&format!("{base}/api/sites"), get(sites))
        .route(&format!("{base}/api/search"), get(search))
        .route(
            &format!("{base}/api/pages/{{site}}"),
            get(get_index).put(put_index),
        )
        .route(
            &format!("{base}/api/pages/{{site}}/{{*path}}"),
            get(get_page).put(put_page),
        )
        .route(
            &format!("{base}/api/files/{{site}}/{{*path}}"),
            get(get_file).put(put_file),
        )
        .route(
            &format!("{base}/api/project/files/{{*path}}"),
            get(get_project_file).put(put_project_file),
        )
        .route(&format!("{base}/api/project"), get(project))
        .route(
            &format!("{base}/api/data/{{*path}}"),
            get(get_data).put(put_data),
        )
        .route(&format!("{base}/api/actions/{{site}}"), get(list_actions))
        .route(
            &format!("{base}/api/actions/{{site}}/{{recipe}}"),
            post(run_action),
        )
        .route(
            &format!("{base}/api/sessions/{{id}}/messages"),
            post(post_message),
        )
        .route_layer(middleware::from_fn_with_state(server, require_app_token))
}

/// Requires the token the app was opened with (`?app_token=` once, then its cookie; or
/// `Authorization: Bearer`), for this project and this app. Like the workspace's, a wrong token
/// is a 404: it learns nothing. A `?app_token=` sets the cookie, scoped to the app's path.
async fn require_app_token(
    State(server): State<Arc<Server>>,
    params: RawPathParams,
    mut request: Request,
    next: Next,
) -> Response {
    let param = |name: &str| {
        params
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.to_string())
            .unwrap_or_default()
    };
    let (project, app) = (param("project"), param("app"));
    let headers = request.headers();
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    let query = request
        .uri()
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == "app_token")
        .map(|(_, v)| v.to_string());
    let cookie = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .filter(|(k, _)| *k == APP_COOKIE)
        .map(|(_, v)| v.to_string())
        .find(|t| fits(&server, t, &project, &app));
    let token = [bearer, query.clone()]
        .into_iter()
        .flatten()
        .find(|t| fits(&server, t, &project, &app))
        .or(cookie.clone());
    let Some(grant) = token.as_deref().and_then(|t| server.app_tokens.grant(t)) else {
        return (
            StatusCode::NOT_FOUND,
            "not found (or not with this app's token: open the app from its project to get one)",
        )
            .into_response();
    };
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
        && !same_origin(origin, headers)
    {
        return (StatusCode::FORBIDDEN, "cross-origin request").into_response();
    }
    let secure = is_https(headers);
    request.extensions_mut().insert(grant);
    let mut response = next.run(request).await;
    if let Some(token) = query.filter(|q| cookie.as_deref() != Some(q.as_str())) {
        let path = format!(
            "{}/p/{}/{}/",
            server.route_base(),
            encode_segment(&project),
            encode_segment(&app)
        );
        let mut value = format!("{APP_COOKIE}={token}; Path={path}; HttpOnly; SameSite=Strict");
        if secure {
            value.push_str("; Secure");
        }
        if let Ok(value) = header::HeaderValue::from_str(&value) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

/// `token` is an app token for `app` on `project`.
fn fits(server: &Server, token: &str, project: &str, app: &str) -> bool {
    server
        .app_tokens
        .grant(token)
        .is_some_and(|g| g.project.as_str() == project && g.site == app)
}

/// `name` as one URL path segment.
pub fn encode_segment(name: &str) -> String {
    name.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// What an app's token can't do: the message an app (or the agent that wrote it) reads.
fn refused(grant: &AppGrant, what: &str) -> ApiError {
    ApiError(
        StatusCode::FORBIDDEN,
        format!(
            "the {} app (opened on project `{}`) can't {what}: an app reads any page but writes \
             only site://{} and its project, and runs only site://{}'s actions",
            grant.site, grant.project, grant.site, grant.site
        ),
    )
}

fn own_site(grant: &AppGrant, site: &str, what: &str) -> Result<(), ApiError> {
    let site = SiteUrl::new(site, "")?.site().to_string();
    if site == grant.site {
        Ok(())
    } else {
        Err(refused(grant, &format!("{what} site://{site}")))
    }
}

async fn redirect(Path((_, app)): Path<(String, String)>, uri: axum::http::Uri) -> Redirect {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    Redirect::permanent(&format!("{}/{query}", encode_segment(&app)))
}

async fn index(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
) -> Response {
    app_routes::serve_app(&server, &grant.site, "")
}

async fn file(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path((_, _, path)): Path<(String, String, String)>,
) -> Response {
    app_routes::serve_app(&server, &grant.site, &path)
}

async fn ws(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade.on_upgrade(move |socket| ws_routes::run(server, socket, Some(grant)))
}

async fn sites(state: State<Arc<Server>>) -> Result<Json<Vec<SiteSummary>>, ApiError> {
    http_routes::sites(state).await
}

async fn search(
    state: State<Arc<Server>>,
    query: Query<HashMap<String, String>>,
) -> Result<Json<Vec<SearchHit>>, ApiError> {
    http_routes::search(state, query).await
}

async fn get_index(
    State(server): State<Arc<Server>>,
    Path((_, _, site)): Path<(String, String, String)>,
) -> Result<Json<PageResponse>, ApiError> {
    http_routes::read(&server, SiteUrl::new(&site, "")?)
}

async fn get_page(
    State(server): State<Arc<Server>>,
    Path((_, _, site, path)): Path<(String, String, String, String)>,
) -> Result<Json<PageResponse>, ApiError> {
    http_routes::read(&server, SiteUrl::new(&site, &path)?)
}

async fn put_index(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path((_, _, site)): Path<(String, String, String)>,
    Json(body): Json<PutPageRequest>,
) -> Result<Json<PutPageResponse>, ApiError> {
    own_site(&grant, &site, "write to")?;
    http_routes::write(&server, SiteUrl::new(&site, "")?, body)
}

async fn put_page(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path((_, _, site, path)): Path<(String, String, String, String)>,
    Json(body): Json<PutPageRequest>,
) -> Result<Json<PutPageResponse>, ApiError> {
    own_site(&grant, &site, "write to")?;
    http_routes::write(&server, SiteUrl::new(&site, &path)?, body)
}

async fn get_file(
    state: State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path((_, _, site, path)): Path<(String, String, String, String)>,
) -> Result<Response, ApiError> {
    own_site(&grant, &site, "read the files of")?;
    app_routes::get_file(state, Path((site, path))).await
}

async fn put_file(
    state: State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path((_, _, site, path)): Path<(String, String, String, String)>,
    body: Bytes,
) -> Result<Json<PutPageResponse>, ApiError> {
    own_site(&grant, &site, "write to")?;
    app_routes::put_file(state, Path((site, path)), body).await
}

/// The sdk's `project()`: the project the app was opened on.
async fn project(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
) -> Json<ProjectInfo> {
    let info = tokio::task::spawn_blocking(move || crate::apps::project_info(&server, &grant));
    Json(info.await.expect("project info doesn't panic"))
}

async fn get_project_file(
    Extension(grant): Extension<AppGrant>,
    Path((_, _, path)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let what = format!("project `{}`", grant.project);
    read_in(&inside(&grant.folder, &path, &what)?, &path, &what)
}

async fn put_project_file(
    Extension(grant): Extension<AppGrant>,
    Path((_, _, path)): Path<(String, String, String)>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let what = format!("project `{}`", grant.project);
    write_in(&inside(&grant.folder, &path, &what)?, &path, &body)
}

/// The app's work data on its project (`APP_DATA` for its recipes): `GET <app>/api/data/<path>`.
async fn get_data(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path((_, _, path)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let data = data_folder(&server, &grant)?;
    let what = format!("the {} app's data on `{}`", grant.site, grant.project);
    read_in(&inside(&data, &path, &what)?, &path, &what)
}

async fn put_data(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path((_, _, path)): Path<(String, String, String)>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let data = data_folder(&server, &grant)?;
    let what = format!("the {} app's data on `{}`", grant.site, grant.project);
    write_in(&inside(&data, &path, &what)?, &path, &body)
}

fn data_folder(server: &Server, grant: &AppGrant) -> Result<PathBuf, ApiError> {
    let data = grant.data(&server.home);
    std::fs::create_dir_all(&data).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("{}: {e}", data.display()),
        )
    })?;
    Ok(data)
}

fn read_in(file: &FsPath, path: &str, what: &str) -> Result<Response, ApiError> {
    let bytes = std::fs::read(file).map_err(|_| {
        ApiError(
            StatusCode::NOT_FOUND,
            format!("{path} isn't a file in {what}"),
        )
    })?;
    Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
}

fn write_in(file: &FsPath, path: &str, body: &[u8]) -> Result<StatusCode, ApiError> {
    let write = || -> std::io::Result<()> {
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, body)
    };
    write().map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("{path}: {e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// `path` inside `folder` (`what`, for the message): relative, no `..`, and no symlink on the
/// way that leads out.
fn inside(folder: &FsPath, path: &str, what: &str) -> Result<PathBuf, ApiError> {
    let relative = FsPath::new(path);
    let plain = relative
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    let outside = || {
        ApiError(
            StatusCode::FORBIDDEN,
            format!("{path} isn't inside {what}: give a path relative to it, without `..`"),
        )
    };
    if !plain || path.is_empty() {
        return Err(outside());
    }
    let root = std::fs::canonicalize(folder).map_err(|_| outside())?;
    let file = root.join(relative);
    // The deepest part that exists, symlinks resolved, must still be in the project.
    let existing = file
        .ancestors()
        .find(|a| a.exists())
        .and_then(|a| std::fs::canonicalize(a).ok())
        .ok_or_else(outside)?;
    if !existing.starts_with(&root) {
        return Err(outside());
    }
    Ok(file)
}

async fn list_actions(
    state: State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path((_, _, site)): Path<(String, String, String)>,
) -> Result<Json<Vec<ActionSummary>>, ApiError> {
    own_site(&grant, &site, "list the actions of")?;
    action_routes::list(state, Path(site)).await
}

async fn run_action(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path((_, _, site, recipe)): Path<(String, String, String, String)>,
    body: Option<Json<RunActionRequest>>,
) -> Result<Json<RunActionResponse>, ApiError> {
    own_site(&grant, &site, "run the actions of")?;
    let Json(request) = body.unwrap_or_default();
    action_routes::run_action(server, site, recipe, request, Some(grant)).await
}

/// A message into a session, marked as coming from this app (whatever site it claims).
async fn post_message(
    state: State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path((_, _, id)): Path<(String, String, String)>,
    Json(mut body): Json<AppMessage>,
) -> Result<StatusCode, ApiError> {
    body.site = Some(grant.site.clone());
    http_routes::post_message(state, Path(id), Json(body)).await
}
