//! Site actions for micro apps: the registry the `actions` and `run` tools speak, over HTTP.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use composer_core::SiteUrl;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::actions::{
    Caller, Justfile, RecipeEnv, Refusal, check_args, ensure_approved, run_recipe,
};
use crate::api::ApiError;
use crate::server::Server;

#[derive(Serialize)]
pub struct ActionInfo {
    name: String,
    doc: Option<String>,
    /// `name`, `name=default` or `*name`, as in `actions`.
    parameters: Vec<String>,
    approved: bool,
}

#[derive(Deserialize, Default)]
pub struct RunRequest {
    /// Recipe arguments, in order.
    #[serde(default)]
    args: Vec<String>,
    /// The session whose clients are asked when the recipe needs approval (an app's URL carries
    /// `session=<id>`). Without one, a recipe that needs approval is not run.
    session: Option<String>,
}

#[derive(Serialize)]
pub struct RunResponse {
    /// `ok`, `failed`, `timed out` or `killed`.
    status: String,
    exit_code: Option<i32>,
    output: String,
}

/// `GET /api/actions/<site>`: the site's public recipes, with whether apps may run them unasked.
pub async fn list(
    State(server): State<Arc<Server>>,
    Path(site): Path<String>,
) -> Result<Json<Vec<ActionInfo>>, ApiError> {
    let justfile = load(&server, &site).await?;
    let site = SiteUrl::new(&site, "")?.site().to_string();
    Ok(Json(
        justfile
            .recipes()
            .into_iter()
            .filter(|r| !r.private)
            .map(|r| ActionInfo {
                approved: server
                    .approvals
                    .may_run(&site, &r, &justfile.hash(&r.name), Caller::App),
                name: r.name,
                doc: r.doc,
                parameters: r.parameters,
            })
            .collect(),
    ))
}

/// `POST /api/actions/<site>/<recipe>` with `{ "args": [...], "session": "<id>" }`: runs the
/// recipe and answers with its output. A recipe version not approved for apps (an approval for an
/// agent's `run` doesn't count, unless the recipe is marked `[metadata('app')]`) asks the session's
/// clients and waits; with no session (or when denied) it answers 403 and does not run.
pub async fn run(
    State(server): State<Arc<Server>>,
    Path((site, recipe)): Path<(String, String)>,
    body: Option<Json<RunRequest>>,
) -> Result<Json<RunResponse>, ApiError> {
    let Json(request) = body.unwrap_or_default();
    let justfile = load(&server, &site).await?;
    let site = SiteUrl::new(&site, "")?.site().to_string();
    let found = justfile
        .recipe(&recipe)
        .filter(|r| !r.private)
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                format!("no action `{recipe}` in site://{site}"),
            )
        })?;
    check_args(&found, &request.args, true)
        .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    let session = match &request.session {
        Some(id) => Some(server.sessions.get(id)?),
        None => None,
    };
    let cancel = CancellationToken::new();

    if !server
        .approvals
        .may_run(&site, &found, &justfile.hash(&found.name), Caller::App)
    {
        let Some(session) = &session else {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                format!(
                    "`{}` needs the user's approval for apps before it runs (one for an agent's \
                     `run` doesn't count); pass `session` to ask them",
                    found.name
                ),
            ));
        };
        ensure_approved(
            &server,
            session,
            &site,
            &justfile,
            &found,
            &request.args,
            Caller::App,
            &cancel,
        )
        .await
        .map_err(|refusal| match refusal {
            Refusal::Denied(m) => ApiError(StatusCode::FORBIDDEN, m),
            Refusal::Changed(m) => ApiError(StatusCode::CONFLICT, m),
        })?;
    }

    let env = RecipeEnv {
        site: site.clone(),
        session: request.session.clone().unwrap_or_default(),
        project: session.as_ref().and_then(|s| s.info().project),
    };
    let timeout = Duration::from_secs(server.config().agent.recipe_timeout_secs);
    let output = run_recipe(
        &justfile,
        &found.name,
        &request.args,
        &env,
        timeout,
        &cancel,
    )
    .await
    .map_err(unprocessable)?;
    let status = match (output.exit_code, output.timed_out) {
        (_, true) => "timed out",
        (Some(0), _) => "ok",
        (Some(_), _) => "failed",
        (None, _) => "killed",
    };
    Ok(Json(RunResponse {
        status: status.into(),
        exit_code: output.exit_code,
        output: output.output,
    }))
}

async fn load(server: &Server, site: &str) -> Result<Justfile, ApiError> {
    let url = SiteUrl::new(site, "")?;
    Justfile::load(&server.workspace.root().join(url.site()))
        .await
        .map_err(|e| ApiError(StatusCode::NOT_FOUND, e))
}

fn unprocessable(error: String) -> ApiError {
    ApiError(StatusCode::UNPROCESSABLE_ENTITY, error)
}
