use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use composer_protocol::http::HookReply;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::api::ApiError;
use crate::runs::{RunRequest, StoredRun};
use crate::server::Server;

/// `POST /api/hooks/{name}`: a webhook (`[hooks.<name>]`). Authenticated by that hook's own
/// bearer token only, never a device's or the local one. Starts a run of the hook's kind on the
/// body and waits for its result; a stored run (`store = true`) also gives its session's id.
pub async fn call(
    State(server): State<Arc<Server>>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<HookReply>, ApiError> {
    let Some(hook) = server.config().hooks.get(&name).cloned() else {
        return Err(ApiError(StatusCode::NOT_FOUND, format!("no hook `{name}`")));
    };
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !hook.accepts(presented) {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "not this hook's token".into(),
        ));
    }
    // A JSON body, or plain text taken as one string.
    let body = match serde_json::from_slice::<Value>(&body) {
        Ok(value) => value,
        Err(_) => Value::String(String::from_utf8_lossy(&body).into_owned()),
    };
    let input = hook
        .input(&body)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    let request = RunRequest {
        kind: hook.kind,
        site: hook.site.clone(),
        folder: hook.folder.clone(),
        input,
    }
    .checked(&server.web)
    .map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("hook `{name}`: {e}"),
        )
    })?;
    let timeout = Duration::from_secs(hook.timeout_secs);
    let failed = |e: String| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e);
    let timed_out = || {
        ApiError(
            StatusCode::GATEWAY_TIMEOUT,
            format!(
                "the run took longer than {}s and was stopped",
                hook.timeout_secs
            ),
        )
    };

    if hook.store {
        let run = StoredRun::start(&server, &request, None, true).map_err(failed)?;
        let session = run.session.clone();
        return match tokio::time::timeout(timeout, run.result()).await {
            Ok(result) => Ok(Json(HookReply {
                value: result.map_err(failed)?,
                session: Some(session.id().to_string()),
            })),
            Err(_) => {
                session.cancel(None);
                Err(timed_out())
            }
        };
    }
    let cancel = CancellationToken::new();
    match tokio::time::timeout(timeout, request.run_unstored(&server, &cancel)).await {
        Ok(result) => Ok(Json(HookReply {
            value: result.map_err(failed)?,
            session: None,
        })),
        Err(_) => {
            cancel.cancel();
            Err(timed_out())
        }
    }
}
