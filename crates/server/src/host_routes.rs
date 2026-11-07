//! The host's own routes: `/api/workspaces` (list, and create with the master token), and every
//! `/w/<name>/...` request handed to that workspace's router, which may have been added while
//! serving.

use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get};
use serde::Deserialize;
use serde_json::json;
use tower::ServiceExt;

use crate::host_state::HostState;
use crate::template_name::TemplateName;
use crate::workspace_name::WorkspaceName;

/// `GET /api/workspaces`: the workspaces the presented token opens, as `[{name, base}]`. The
/// master token lists all of them; a workspace's (or its devices') token lists that one; any
/// other gets 404, like everything else it can't open.
///
/// `POST /api/workspaces` with `{name, template?}`, master token only: creates the workspace and
/// serves it right away; the answer is `{name, base}`.
pub fn workspaces(state: Arc<HostState>, master: String) -> MethodRouter {
    let listing = (state.clone(), master.clone());
    get(move |headers: HeaderMap, uri: Uri| async move {
        let (state, master) = listing;
        let Some(token) = presented_token(&headers, &uri) else {
            return not_found();
        };
        let open: Vec<_> = state
            .list()
            .into_iter()
            .filter(|(_, server)| token == master || server.accepts_token(&token))
            .map(
                |(name, server)| json!({ "name": name.as_str(), "base": server.home.route_base() }),
            )
            .collect();
        if open.is_empty() {
            return not_found();
        }
        Json(open).into_response()
    })
    .post(
        move |headers: HeaderMap, uri: Uri, body: Json<NewWorkspace>| async move {
            if presented_token(&headers, &uri).as_deref() != Some(master.as_str()) {
                return not_found();
            }
            create(&state, body.0).await
        },
    )
}

#[derive(Deserialize)]
pub struct NewWorkspace {
    name: String,
    #[serde(default)]
    template: Option<String>,
}

async fn create(state: &HostState, request: NewWorkspace) -> Response {
    let name = match WorkspaceName::new(&request.name) {
        Ok(name) => name,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let template = match request
        .template
        .as_deref()
        .map(TemplateName::new)
        .transpose()
    {
        Ok(template) => template,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    match state.create(&name, template.as_ref()).await {
        Ok(server) => (
            StatusCode::CREATED,
            Json(json!({ "name": name.as_str(), "base": server.home.route_base() })),
        )
            .into_response(),
        Err(e) => (StatusCode::CONFLICT, e.to_string()).into_response(),
    }
}

/// `/w/<name>/<rest>` → workspace `name`'s router, as `/<rest>` (`/w/<name>` alone is `/`).
/// Anything else, or a workspace the host doesn't serve, is 404.
pub async fn dispatch(state: Arc<HostState>, mut request: Request) -> Response {
    let path = request.uri().path().to_string();
    let Some(after) = path.strip_prefix("/w/") else {
        return not_found();
    };
    let (name, rest) = after.split_once('/').unwrap_or((after, ""));
    let Some(router) = state.route(name) else {
        return not_found();
    };
    let rest = format!("/{rest}");
    let path_and_query = match request.uri().query() {
        Some(query) => format!("{rest}?{query}"),
        None => rest,
    };
    let mut parts = request.uri().clone().into_parts();
    parts.path_and_query = path_and_query.parse().ok();
    let Ok(uri) = Uri::from_parts(parts) else {
        return not_found();
    };
    *request.uri_mut() = uri;
    match router.oneshot(request.map(Body::new)).await {
        Ok(response) => response,
        Err(never) => match never {},
    }
}

/// The token from `Authorization: Bearer` or `?token=`.
fn presented_token(headers: &HeaderMap, uri: &Uri) -> Option<String> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let query = uri
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == "token")
        .map(|(_, v)| v);
    bearer.or(query).map(str::to_string)
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}
