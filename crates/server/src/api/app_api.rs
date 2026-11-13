//! An app's own routes: `<base>/a/<app token>/`, for an app opened on a project (ticket 0163) or
//! on Home (ticket 0168). They serve the app, and `.../api/...` and `.../ws` are the API as its
//! token allows (decision 2026-11-07): read any page of the workspace; write only its own site and
//! its project (on Home: no project); run only its own site's actions. These routes sit outside the
//! workspace token's: an app page never needs (and never gets) it.
//!
//! An app's code is untrusted (an agent may have written it), so every response here carries
//! [`SANDBOX`]: the page runs in an opaque origin. Its requests say `Origin: null` and, being
//! cross-site, carry no SameSite=Strict cookie, so the workspace's cookie is out of its reach
//! whatever the page does. That is why the token is in the path: the app's relative URLs (and its
//! socket) carry it without a cookie or a header. The answers allow `Origin: null` to read them
//! (CORS), nothing else; the Origin rule is [`crate::api::auth::origin_refused`].

use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, RawPathParams, Request, State, WebSocketUpgrade};
use axum::http::{HeaderValue, Method, StatusCode, header};
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
use crate::api::auth::{Audience, origin_refused};
use crate::api::{ApiError, action_routes, app_routes, http_routes, ws_routes};
use crate::apps::AppGrant;
use crate::project::ProjectMode;
use crate::server::Server;

/// Where apps are served: `<base>/a/<app token>/` (a site named `a` is not served as a client
/// app, like `h` and `p`).
pub const APP_PREFIX: &str = "/a";

/// The sandbox every app response is served in: scripts, forms, dialogs, popups (which inherit
/// the sandbox) and downloads (an app's exports), and no `allow-same-origin`: an opaque origin.
pub const SANDBOX: &str =
    "sandbox allow-scripts allow-forms allow-popups allow-modals allow-downloads";

/// A route's named parameters (`token`, `site`, `path`, ...).
type Params = HashMap<String, String>;

fn arg(params: &Params, name: &str) -> String {
    params.get(name).cloned().unwrap_or_default()
}

/// The routes, to merge into a workspace's router (outside its token layer).
pub fn router(server: Arc<Server>) -> Router<Arc<Server>> {
    // A layer, not a route layer: it also answers the CORS preflight (`OPTIONS`) of every route.
    routes(Router::new(), &format!("{APP_PREFIX}/{{token}}"))
        .layer(middleware::from_fn_with_state(server, require_app_token))
        // The old forms, `/p/<project>/<app>/?app_token=` and `/h/<app>/?app_token=`: to the new.
        .route("/p/{project}/{app}", get(old_url))
        .route("/p/{project}/{app}/", get(old_url))
        .route("/h/{app}", get(old_url))
        .route("/h/{app}/", get(old_url))
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
        .route(&format!("{base}/api/selection"), post(select))
        .route(&format!("{base}/api/events"), post(app_event))
}

/// Requires the app token in the path (`<base>/a/<token>/...`); nothing else authenticates here
/// (no cookie, no header). Like the workspace's, a wrong token is a 404: it learns nothing. Every
/// answer is sandboxed ([`SANDBOX`]) and readable by the app's opaque origin; a CORS preflight is
/// answered here.
async fn require_app_token(
    State(server): State<Arc<Server>>,
    params: RawPathParams,
    mut request: Request,
    next: Next,
) -> Response {
    let token = params
        .iter()
        .find(|(k, _)| *k == "token")
        .map(|(_, v)| v.to_string())
        .unwrap_or_default();
    let null_origin = request
        .headers()
        .get(header::ORIGIN)
        .is_some_and(|o| o.as_bytes() == b"null");
    let refused = origin_refused(request.headers(), Audience::App);
    let mut response = match (server.app_tokens.grant(&token), refused) {
        (None, _) => (
            StatusCode::NOT_FOUND,
            "not found (or not with this app's token: open the app to get one)",
        )
            .into_response(),
        (Some(_), Some(refused)) => refused,
        (Some(_), None) if request.method() == Method::OPTIONS => preflight(),
        (Some(grant), None) => {
            request.extensions_mut().insert(grant.clone());
            let response = next.run(request).await;
            with_meta(response, &grant).await
        }
    };
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(SANDBOX),
    );
    if null_origin {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("null"),
        );
    }
    response
}

/// The answer to a CORS preflight from the app's page: what the sdk sends.
fn preflight() -> Response {
    (
        StatusCode::NO_CONTENT,
        [
            (header::ACCESS_CONTROL_ALLOW_METHODS, "GET, PUT, POST"),
            (header::ACCESS_CONTROL_ALLOW_HEADERS, "content-type"),
            (header::ACCESS_CONTROL_MAX_AGE, "600"),
        ],
    )
        .into_response()
}

/// An app's HTML page gets what the sdk can no longer read from its URL: its site, its project
/// and the session it was opened from, in `<meta name="composer-app">`.
async fn with_meta(response: Response, grant: &AppGrant) -> Response {
    let html = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|t| t.as_bytes().starts_with(b"text/html"));
    if !html || response.status() != StatusCode::OK {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, 64 * 1024 * 1024).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let facts = serde_json::json!({
        "site": grant.site,
        "project": grant.project.as_ref().map(|p| p.name.to_string()),
        "session": grant.session,
    })
    .to_string();
    let meta = format!(
        "<meta name=\"composer-app\" content=\"{}\">",
        facts
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    );
    let page = String::from_utf8_lossy(&bytes);
    let at = page
        .to_ascii_lowercase()
        .find("<head>")
        .map(|at| at + "<head>".len())
        .unwrap_or(0);
    let page = format!("{}{meta}{}", &page[..at], &page[at..]);
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, axum::body::Body::from(page))
}

/// `<base>/p/<project>/<app>/?app_token=<t>` and `<base>/h/<app>/?app_token=<t>` (the URLs
/// before the token moved into the path): to `<base>/a/<t>/`, with the rest of the query, only
/// for a token of that app there. Nothing else is served at the old URLs.
async fn old_url(
    State(server): State<Arc<Server>>,
    Path(p): Path<Params>,
    uri: axum::http::Uri,
) -> Response {
    let project = p.get("project").map(String::as_str);
    let app = arg(&p, "app");
    let pairs: Vec<(&str, &str)> = uri
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .collect();
    let token = pairs
        .iter()
        .find(|(k, _)| *k == "app_token")
        .map(|(_, v)| *v)
        .filter(|t| fits(&server, t, project, &app));
    let Some(token) = token else {
        return (
            StatusCode::NOT_FOUND,
            "not found (apps are served at <base>/a/<app token>/ now: open the app again)",
        )
            .into_response();
    };
    let rest: Vec<String> = pairs
        .iter()
        .filter(|(k, _)| *k != "app_token")
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    let query = match rest.is_empty() {
        true => String::new(),
        false => format!("?{}", rest.join("&")),
    };
    Redirect::to(&format!(
        "{}{APP_PREFIX}/{token}/{query}",
        server.route_base()
    ))
    .into_response()
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
    Redirect::permanent(&format!("{}/{query}", arg(&p, "token")))
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
         know how apps are served now (sandboxed, at <code>&lt;base&gt;/a/&lt;app token&gt;/</code>): \
         rebuild it (<code>just ui-build</code> in the site, its <code>ui-build</code> action, or \
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
    upgrade.on_upgrade(move |socket| ws_routes::run(server, socket, ws_routes::Client::App(grant)))
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
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let path = arg(&p, "path");
    let (root, _) = project_root(&server, &grant, query.get("work").map(String::as_str))?;
    let what = format!("the {} app's project", grant.site);
    read_in(&inside(&root, &path, &what)?, &path, &what)
}

async fn put_project_file(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
    Query(query): Query<HashMap<String, String>>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let path = arg(&p, "path");
    let (root, mode) = project_root(&server, &grant, query.get("work").map(String::as_str))?;
    if mode.readonly() {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            format!("the {} work is read-only", grant.site),
        ));
    }
    let what = format!("the {} app's project", grant.site);
    write_in(&inside(&root, &path, &what)?, &path, &body)
}

/// The project folder an app reads/writes: the main folder, or the work's `project/` (which is
/// the project root or its worktree). A work must belong to the project the app was opened on.
fn project_root(
    _server: &Server,
    grant: &AppGrant,
    work: Option<&str>,
) -> Result<(PathBuf, ProjectMode), ApiError> {
    let project = granted(grant)?;
    if work.is_some_and(|w| !w.trim().is_empty()) {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "works are not a server concept: an app reads its project directly or a site's JSON"
                .into(),
        ));
    }
    Ok((project.folder.clone(), ProjectMode::Write))
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

/// A message into the session the app was opened from (ticket 0172), marked as coming from this
/// app (whatever site it claims). Another session, or none: refused.
async fn post_message(
    state: State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Path(p): Path<Params>,
    Json(mut body): Json<AppMessage>,
) -> Result<StatusCode, ApiError> {
    let id = arg(&p, "id");
    if grant.session.as_deref() != Some(id.as_str()) {
        let opened = match &grant.session {
            Some(own) => format!("from session `{own}`"),
            None => "outside a chat".to_string(),
        };
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            format!(
                "the {} app was opened {opened}: it messages only the session it was opened from \
                 (open it from that chat: the TUI's `a`, or `app.open` with `session`)",
                grant.site
            ),
        ));
    }
    body.site = Some(grant.site.clone());
    http_routes::post_message(state, Path(id), Json(body)).await
}

/// The session an app was opened from, which its selection and events go to (tickets 0172,
/// 0175); refused for an app opened outside a chat.
fn own_session(
    server: &Server,
    grant: &AppGrant,
) -> Result<Arc<crate::session::Session>, ApiError> {
    let id = grant.session.as_deref().ok_or_else(|| {
        ApiError(
            StatusCode::FORBIDDEN,
            format!(
                "the {} app was opened outside a chat: open it from one to point the agent at \
                 things",
                grant.site
            ),
        )
    })?;
    server
        .sessions
        .get(id)
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, format!("session `{id}` is gone")))
}

/// What the app's selection says (the sdk's `select`); `label: null` clears it.
#[derive(serde::Deserialize)]
struct SelectBody {
    label: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

/// `POST <app>/api/selection`: what the user points at in the app becomes the session's chip.
async fn select(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Json(body): Json<SelectBody>,
) -> Result<StatusCode, ApiError> {
    let session = own_session(&server, &grant)?;
    let selection = body.label.map(|label| composer_protocol::Selection {
        app: grant.site.clone(),
        label,
        text: body.text,
        url: body.url,
        data: body.data,
    });
    if session.set_selection(selection.clone()) {
        server
            .hub
            .send(composer_protocol::ServerMessage::SelectionChanged {
                session: session.id().to_string(),
                selection,
            });
    }
    Ok(StatusCode::NO_CONTENT)
}

/// An app's event (the sdk's `emit`).
#[derive(serde::Deserialize)]
struct EventBody {
    kind: String,
    #[serde(default)]
    detail: serde_json::Value,
}

/// `POST <app>/api/events`: something happened in the app; the agent sees it at its next turn
/// (it doesn't start one). At most 20 a minute per app and session.
async fn app_event(
    State(server): State<Arc<Server>>,
    Extension(grant): Extension<AppGrant>,
    Json(body): Json<EventBody>,
) -> Result<StatusCode, ApiError> {
    let session = own_session(&server, &grant)?;
    let kind = body.kind.trim();
    if kind.is_empty() || kind.len() > 64 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "an event's `kind` is a short name (gate.approved, tab.saved)".into(),
        ));
    }
    if !server.app_events.allow(session.id(), &grant.site) {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            format!(
                "the {} app reported more than 20 events this minute",
                grant.site
            ),
        ));
    }
    let detail: String = body.detail.to_string().chars().take(2_000).collect();
    session.add_note(format!("- the {} app: {kind} {detail}", grant.site));
    server.hub.send(composer_protocol::ServerMessage::AppEvent {
        session: session.id().to_string(),
        app: grant.site.clone(),
        kind: kind.to_string(),
        detail: body.detail,
    });
    Ok(StatusCode::NO_CONTENT)
}
