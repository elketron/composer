use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use composer_protocol::http::{
    PageResponse, ProjectDiff, PutPageRequest, PutPageResponse, SearchHit, ServerInfo, SiteSummary,
};

use crate::api::{ApiError, Credential};
use crate::server::Server;

/// `POST /api/web-session`: a new browser session of the caller's kind (a device gets a device's
/// session, the local token the local user's), for a client that sets the cookie in its own
/// webview (the desktop's panes, the phone's live view).
pub async fn web_session(
    State(server): State<Arc<Server>>,
    Extension(credential): Extension<Credential>,
) -> Json<composer_protocol::http::WebSession> {
    Json(composer_protocol::http::WebSession {
        cookie_name: crate::api::auth::cookie_name(&server),
        value: server.web_sessions.mint(credential),
        path: "/".into(),
    })
}

/// `GET /api/info`: what this server is.
pub async fn info(State(server): State<Arc<Server>>) -> Json<ServerInfo> {
    let config = server.config();
    Json(ServerInfo {
        name: server.name(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        model: server.provider.model().to_string(),
        slots: config.agent.slots,
        viewer: crate::api::viewer_proxy::viewer_path(&server),
        harnesses: config.acp.keys().cloned().collect(),
    })
}

pub async fn sites(State(server): State<Arc<Server>>) -> Result<Json<Vec<SiteSummary>>, ApiError> {
    let sites = server.web.sites()?;
    let index = server.index.read().expect("index lock");
    let mut pages: HashMap<&str, u32> = HashMap::new();
    for entry in index.entries() {
        *pages.entry(entry.url.site()).or_default() += 1;
    }
    Ok(Json(
        sites
            .into_iter()
            .map(|s| {
                // The servers its `mcp.toml` declares, approved or not.
                let mcp = crate::site_mcp::load(&server.web, &s.name)
                    .map(|file| file.servers.into_iter().map(|s| s.name).collect())
                    .unwrap_or_default();
                SiteSummary {
                    mcp,
                    pages: pages.get(s.name.as_str()).copied().unwrap_or(0),
                    name: s.name,
                    title: s.title,
                    has_justfile: s.has_justfile,
                    description: s.description,
                }
            })
            .collect(),
    ))
}

pub async fn search(
    State(server): State<Arc<Server>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<SearchHit>>, ApiError> {
    let query = composer_core::Query::parse(params.get("q").map(String::as_str).unwrap_or(""));
    let limit = params
        .get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(50);
    let hits = server.index.read().expect("index lock").search(&query);
    Ok(Json(
        hits.into_iter()
            .take(limit)
            .map(|h| SearchHit {
                site: h.url.site().to_string(),
                url: h.url.to_string(),
                title: h.title,
            })
            .collect(),
    ))
}

pub async fn get_site_index(
    State(server): State<Arc<Server>>,
    Path(site): Path<String>,
) -> Result<Json<PageResponse>, ApiError> {
    read(&server, SiteUrl::new(&site, "")?)
}

pub async fn get_page(
    State(server): State<Arc<Server>>,
    Path((site, path)): Path<(String, String)>,
) -> Result<Json<PageResponse>, ApiError> {
    read(&server, SiteUrl::new(&site, &path)?)
}

pub async fn put_site_index(
    State(server): State<Arc<Server>>,
    Path(site): Path<String>,
    Json(body): Json<PutPageRequest>,
) -> Result<Json<PutPageResponse>, ApiError> {
    write(&server, SiteUrl::new(&site, "")?, body)
}

pub async fn put_page(
    State(server): State<Arc<Server>>,
    Path((site, path)): Path<(String, String)>,
    Json(body): Json<PutPageRequest>,
) -> Result<Json<PutPageResponse>, ApiError> {
    write(&server, SiteUrl::new(&site, &path)?, body)
}

pub(crate) fn read(server: &Server, url: SiteUrl) -> Result<Json<PageResponse>, ApiError> {
    let page = server.web.read_page(&url)?;
    let backlinks = server
        .index
        .read()
        .expect("index lock")
        .backlinks(&page.url);
    Ok(Json(PageResponse {
        url: page.url.to_string(),
        title: page.title(),
        frontmatter: serde_json::Value::Object(page.frontmatter.as_map().clone()),
        frontmatter_error: page.frontmatter_error.clone(),
        hash: crate::content_hash(page.raw.as_bytes()),
        backlinks: backlinks.iter().map(ToString::to_string).collect(),
        raw: page.raw,
    }))
}

pub(crate) fn write(
    server: &Server,
    url: SiteUrl,
    body: PutPageRequest,
) -> Result<Json<PutPageResponse>, ApiError> {
    if let Some(expected) = &body.if_match {
        let current = server
            .web
            .locate(&url)
            .ok()
            .and_then(|path| server.page_hash(&path));
        if current.as_ref() != Some(expected) {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "the page changed since you read it".into(),
            ));
        }
    }
    let (written, _) = server.write_page(&url, &body.content, ChangedBy::External)?;
    Ok(Json(PutPageResponse {
        url: written.to_string(),
        hash: crate::content_hash(body.content.as_bytes()),
    }))
}

/// `GET /api/sessions/{id}/project/diff`: git changes of the folder of the session's site.
pub async fn project_diff(
    State(server): State<Arc<Server>>,
    Path(id): Path<String>,
) -> Result<Json<ProjectDiff>, ApiError> {
    let session = server.sessions.get(&id)?;
    let info = session.info();
    let Some(folder) = server.session_folder(&info) else {
        // Why not: no site, no folder named, or one the user has not approved.
        let why = info
            .site
            .as_deref()
            .and_then(|site| crate::agent::Site::load(&server.web, site).ok())
            .filter(|site| site.declared_folder().is_ok())
            .and_then(|site| site.existing_folder(&server.approvals).err())
            .unwrap_or_else(|| {
                "the session's site names no folder (`folder:` in its index.md)".into()
            });
        return Err(ApiError(StatusCode::NOT_FOUND, why));
    };
    let diff = crate::project::project_diff(&folder, None)
        .await
        .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    Ok(Json(diff))
}
