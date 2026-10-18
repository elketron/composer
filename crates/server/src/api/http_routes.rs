use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use composer_core::SiteUrl;
use composer_protocol::http::{
    AppMessage, PageResponse, ProjectDiff, PutPageRequest, PutPageResponse, SearchHit, ServerInfo,
    SiteSummary,
};
use composer_protocol::{ChangedBy, ChatMessage, MessageSource, Role};

use crate::api::ApiError;
use crate::server::Server;
use crate::session::submit;

/// `GET /api/info`: what this server is.
pub async fn info(State(server): State<Arc<Server>>) -> Json<ServerInfo> {
    Json(ServerInfo {
        name: server.name(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        model: server.provider.model().to_string(),
        slots: server.config.agent.slots,
    })
}

pub async fn sites(State(server): State<Arc<Server>>) -> Result<Json<Vec<SiteSummary>>, ApiError> {
    let sites = server.workspace.sites()?;
    Ok(Json(
        sites
            .into_iter()
            .map(|s| SiteSummary {
                name: s.name,
                title: s.title,
                has_justfile: s.has_justfile,
                has_ui: s.has_ui,
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

/// A micro app pushing a message into a session.
pub async fn post_message(
    State(server): State<Arc<Server>>,
    Path(id): Path<String>,
    Json(body): Json<AppMessage>,
) -> Result<StatusCode, ApiError> {
    let session = server.sessions.get(&id)?;
    let message = ChatMessage {
        role: Role::User,
        content: body.text,
        thinking: None,
        tool_calls: vec![],
        pages: body.pages,
        images: body.images,
        source: Some(MessageSource::App {
            site: body.site.unwrap_or_else(|| "unknown".into()),
        }),
    };
    submit(&server, &session, message);
    Ok(StatusCode::ACCEPTED)
}

fn read(server: &Server, url: SiteUrl) -> Result<Json<PageResponse>, ApiError> {
    let page = server.workspace.read_page(&url)?;
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

fn write(
    server: &Server,
    url: SiteUrl,
    body: PutPageRequest,
) -> Result<Json<PutPageResponse>, ApiError> {
    if let Some(expected) = &body.if_match {
        let current = server
            .workspace
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
    let (written, _) = server.write_page(&url, &body.content, ChangedBy::App)?;
    Ok(Json(PutPageResponse {
        url: written.to_string(),
        hash: crate::content_hash(body.content.as_bytes()),
    }))
}

/// `GET /api/sessions/{id}/project/diff`: git changes of the session's active project.
pub async fn project_diff(
    State(server): State<Arc<Server>>,
    Path(id): Path<String>,
) -> Result<Json<ProjectDiff>, ApiError> {
    let session = server.sessions.get(&id)?;
    let Some(project) = session.info().project else {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "no active project; set one with :project <path>".into(),
        ));
    };
    let diff = crate::project::project_diff(std::path::Path::new(&project))
        .await
        .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    Ok(Json(diff))
}
