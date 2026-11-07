//! An app's own routes: `<base>/p/<project>/<app>/` for an app opened on a project (ticket 0163),
//! `<base>/h/<app>/` for one opened on Home (ticket 0168). They serve the app, and `.../api/...` is
//! the API as its token allows (decision 2026-11-07): read any page of the workspace; write only
//! its own site and its project (on Home: no project); run only its own site's actions. These
//! routes sit outside the workspace token's: an app page never needs (and never gets) it.

use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
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

use crate::actions::{Justfile, RecipeEnv, check_args, run_recipe};
use crate::api::auth::{is_https, same_origin};
use crate::api::{ApiError, action_routes, app_routes, http_routes, ws_routes};
use crate::apps::AppGrant;
use crate::server::Server;

/// The cookie an app page's token rides in, scoped to the app's own path.
pub const APP_COOKIE: &str = "composer_app";

/// A route's named parameters (`project`, `app`, `site`, `path`, ...).
type Params = HashMap<String, String>;

fn arg(params: &Params, name: &str) -> String {
    params.get(name).cloned().unwrap_or_default()
}

/// The routes, to merge into a workspace's router (outside its token layer).
pub fn router(server: Arc<Server>) -> Router<Arc<Server>> {
    let router = routes(Router::new(), "/p/{project}/{app}");
    routes(router, "/h/{app}")
        .route_layer(middleware::from_fn_with_state(server, require_app_token))
}

/// The app routes under `base`.
fn routes(router: Router<Arc<Server>>, base: &str) -> Router<Arc<Server>> {
    router
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
    // No `project`: an app opened on Home.
    let (project, app) = (param("project"), param("app"));
    let project = (!project.is_empty()).then_some(project);
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
        .find(|t| fits(&server, t, project.as_deref(), &app));
    let token = [bearer, query.clone()]
        .into_iter()
        .flatten()
        .find(|t| fits(&server, t, project.as_deref(), &app))
        .or(cookie.clone());
    let Some(grant) = token.as_deref().and_then(|t| server.app_tokens.grant(t)) else {
        return (
            StatusCode::NOT_FOUND,
            "not found (or not with this app's token: open the app to get one)",
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
        let path = match &project {
            Some(project) => format!(
                "{}/p/{}/{}/",
                server.route_base(),
                encode_segment(project),
                encode_segment(&app)
            ),
            None => format!("{}/h/{}/", server.route_base(), encode_segment(&app)),
        };
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

/// `token` is an app token for `app` on `project` (`None`: on Home).
fn fits(server: &Server, token: &str, project: Option<&str>, app: &str) -> bool {
    server
        .app_tokens
        .grant(token)
        .is_some_and(|g| g.site == app && g.project.as_ref().map(|p| p.name.as_str()) == project)
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
            "the {} app (opened on {}) can't {what}: an app reads any page but writes only \
             site://{} and its project, and runs only site://{}'s actions",
            grant.site,
            grant.place(),
            grant.site,
            grant.site
        ),
    )
}

fn own_site(grant: &AppGrant, site: &str, what: &str) -> Result<(), ApiError> {
    let site = SiteUrl::new(site, "")?.site().to_string();
    if grant.local.is_some() {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            format!(
                "the project-local app `{}` can't {what} site://{site}: it has no site in the web; \
                 it writes its project (`project/files/...`) and its data (`data/...`)",
                grant.site
            ),
        ));
    }
    if site == grant.site {
        Ok(())
    } else {
        Err(refused(grant, &format!("{what} site://{site}")))
    }
}

async fn redirect(Path(p): Path<Params>, uri: axum::http::Uri) -> Redirect {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    Redirect::permanent(&format!("{}/{query}", encode_segment(&arg(&p, "app"))))
}

/// An app built with an older sdk calls the workspace's API from these routes and shows nothing:
/// say so instead.
fn outdated_page(server: &Server, grant: &AppGrant) -> Option<Response> {
    let dir = grant
        .local
        .clone()
        .unwrap_or_else(|| server.web.root().join(&grant.site));
    if !crate::apps::outdated(&dir) {
        return None;
    }
    let site = &grant.site;
    let message = format!(
        "the {site} app was built with an older composer sdk (before version {}), which doesn't \
         know where it is served now: rebuild it (its <code>ui-build</code> action, or \
         <code>composer app rebuild</code> for every app), then reload.",
        crate::apps::SDK_VERSION
    );
    Some(
        (
            StatusCode::CONFLICT,
            axum::response::Html(format!(
                "<!doctype html><meta charset=utf-8><body style=\"background:#0c0d0f;color:#868c96;font:14px monospace;padding:40px\">{message}</body>"
            )),
        )
            .into_response(),
    )
}

async fn index(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
) -> Response {
    if let Some(page) = outdated_page(&server, &grant) {
        return page;
    }
    match &grant.local {
        Some(dir) => app_routes::serve_local(&grant.site, dir, ""),
        None => app_routes::serve_app(&server, &grant.site, ""),
    }
}

async fn file(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
) -> Response {
    let path = arg(&p, "path");
    // A page (not an asset): an outdated build says so.
    let page = path == "index.html" || !path.rsplit('/').next().unwrap_or("").contains('.');
    if let Some(refused) = page.then(|| outdated_page(&server, &grant)).flatten() {
        return refused;
    }
    match &grant.local {
        Some(dir) => app_routes::serve_local(&grant.site, dir, &path),
        None => app_routes::serve_app(&server, &grant.site, &path),
    }
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
    Path(p): Path<Params>,
) -> Result<Json<PageResponse>, ApiError> {
    let site = arg(&p, "site");
    http_routes::read(&server, SiteUrl::new(&site, "")?)
}

async fn get_page(
    State(server): State<Arc<Server>>,
    Path(p): Path<Params>,
) -> Result<Json<PageResponse>, ApiError> {
    let site = arg(&p, "site");
    let path = arg(&p, "path");
    http_routes::read(&server, SiteUrl::new(&site, &path)?)
}

async fn put_index(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
    Json(body): Json<PutPageRequest>,
) -> Result<Json<PutPageResponse>, ApiError> {
    let site = arg(&p, "site");
    own_site(&grant, &site, "write to")?;
    http_routes::write(&server, SiteUrl::new(&site, "")?, body)
}

async fn put_page(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
    Json(body): Json<PutPageRequest>,
) -> Result<Json<PutPageResponse>, ApiError> {
    let site = arg(&p, "site");
    let path = arg(&p, "path");
    own_site(&grant, &site, "write to")?;
    http_routes::write(&server, SiteUrl::new(&site, &path)?, body)
}

async fn get_file(
    state: State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
) -> Result<Response, ApiError> {
    let site = arg(&p, "site");
    let path = arg(&p, "path");
    own_site(&grant, &site, "read the files of")?;
    app_routes::get_file(state, Path((site, path))).await
}

async fn put_file(
    state: State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
    body: Bytes,
) -> Result<Json<PutPageResponse>, ApiError> {
    let site = arg(&p, "site");
    let path = arg(&p, "path");
    own_site(&grant, &site, "write to")?;
    app_routes::put_file(state, Path((site, path)), body).await
}

/// The sdk's `project()`: the project the app was opened on.
async fn project(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
) -> Json<Option<ProjectInfo>> {
    let Some(project) = grant.project else {
        return Json(None);
    };
    let info = tokio::task::spawn_blocking(move || crate::apps::project_info(&server, &project));
    Json(Some(info.await.expect("project info doesn't panic")))
}

/// The project of an app opened on one; on Home, the refusal.
fn granted(grant: &AppGrant) -> Result<&crate::apps::GrantedProject, ApiError> {
    grant.project.as_ref().ok_or_else(|| {
        ApiError(
            StatusCode::FORBIDDEN,
            format!(
                "the {} app is open on Home: it has no project files (open it from a project)",
                grant.site
            ),
        )
    })
}

async fn get_project_file(
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
) -> Result<Response, ApiError> {
    let path = arg(&p, "path");
    let project = granted(&grant)?;
    let what = format!("project `{}`", project.name);
    read_in(&inside(&project.folder, &path, &what)?, &path, &what)
}

async fn put_project_file(
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let path = arg(&p, "path");
    let project = granted(&grant)?;
    let what = format!("project `{}`", project.name);
    write_in(&inside(&project.folder, &path, &what)?, &path, &body)
}

/// The app's work data on its project (`APP_DATA` for its recipes): `GET <app>/api/data/<path>`.
async fn get_data(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
) -> Result<Response, ApiError> {
    let path = arg(&p, "path");
    let data = data_folder(&server, &grant)?;
    let what = format!("the {} app's data on {}", grant.site, grant.place());
    read_in(&inside(&data, &path, &what)?, &path, &what)
}

async fn put_data(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let path = arg(&p, "path");
    let data = data_folder(&server, &grant)?;
    let what = format!("the {} app's data on {}", grant.site, grant.place());
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

/// [`crate::apps::path_inside`], refused as a 403.
fn inside(folder: &FsPath, path: &str, what: &str) -> Result<PathBuf, ApiError> {
    crate::apps::path_inside(folder, path, what).map_err(|e| ApiError(StatusCode::FORBIDDEN, e))
}

async fn list_actions(
    state: State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
) -> Result<Json<Vec<ActionSummary>>, ApiError> {
    let site = arg(&p, "site");
    if let Some(dir) = &grant.local {
        own_actions(&grant, &site)?;
        return local_actions(dir).await;
    }
    own_site(&grant, &site, "list the actions of")?;
    action_routes::list(state, Path(site)).await
}

async fn run_action(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
    body: Option<Json<RunActionRequest>>,
) -> Result<Json<RunActionResponse>, ApiError> {
    let site = arg(&p, "site");
    let recipe = arg(&p, "recipe");
    let Json(request) = body.unwrap_or_default();
    if grant.local.is_some() {
        own_actions(&grant, &site)?;
        return run_local(&server, &grant, &recipe, &request).await;
    }
    own_site(&grant, &site, "run the actions of")?;
    action_routes::run_action(server, site, recipe, request, Some(grant)).await
}

/// A local app runs only its own recipes (`site` is its name).
fn own_actions(grant: &AppGrant, site: &str) -> Result<(), ApiError> {
    if site == grant.site {
        return Ok(());
    }
    Err(ApiError(
        StatusCode::FORBIDDEN,
        format!(
            "the project-local app `{}` runs only its own justfile's recipes, not site://{site}'s",
            grant.site
        ),
    ))
}

async fn local_justfile(dir: &FsPath) -> Result<Justfile, ApiError> {
    Justfile::load(dir)
        .await
        .map_err(|e| ApiError(StatusCode::NOT_FOUND, e))
}

/// A local app's recipes: all approved, since allowing the app was for its justfile.
async fn local_actions(dir: &FsPath) -> Result<Json<Vec<ActionSummary>>, ApiError> {
    let justfile = local_justfile(dir).await?;
    let recipes = justfile.recipes().into_iter().filter(|r| !r.private);
    Ok(Json(
        recipes
            .map(|r| ActionSummary {
                approved: true,
                name: r.name,
                doc: r.doc,
                parameters: r.parameters,
            })
            .collect(),
    ))
}

/// Runs a local app's recipe on its project (sandboxed like any app's; its allowance is the
/// approval).
async fn run_local(
    server: &Arc<Server>,
    grant: &AppGrant,
    recipe: &str,
    request: &RunActionRequest,
) -> Result<Json<RunActionResponse>, ApiError> {
    let dir = grant.local.clone().expect("a local grant");
    let justfile = local_justfile(&dir).await?;
    let found = justfile
        .recipe(recipe)
        .filter(|r| !r.private)
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                format!("no recipe `{recipe}` in the local app `{}`", grant.site),
            )
        })?;
    check_args(&found, &request.args, true)
        .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    let env = RecipeEnv::for_app(server, grant);
    let timeout = std::time::Duration::from_secs(server.config().agent.recipe_timeout_secs);
    let output = run_recipe(
        &justfile,
        &found.name,
        &request.args,
        &env,
        timeout,
        &tokio_util::sync::CancellationToken::new(),
    )
    .await
    .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    let status = match (output.exit_code, output.timed_out) {
        (_, true) => "timed out",
        (Some(0), _) => "ok",
        (Some(_), _) => "failed",
        (None, _) => "killed",
    };
    Ok(Json(RunActionResponse {
        status: status.into(),
        exit_code: output.exit_code,
        output: output.output,
    }))
}

/// A message into a session, marked as coming from this app (whatever site it claims).
async fn post_message(
    state: State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
    Json(mut body): Json<AppMessage>,
) -> Result<StatusCode, ApiError> {
    let id = arg(&p, "id");
    body.site = Some(grant.site.clone());
    http_routes::post_message(state, Path(id), Json(body)).await
}
