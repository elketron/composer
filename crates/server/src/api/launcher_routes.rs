//! The launcher over HTTP: the apps launch button for each `[[launcher]]` entry.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::api::ApiError;
use crate::launcher::{self, LaunchResult, LauncherInfo};
use crate::server::Server;

/// `GET /api/launcher`: the declared entries, for the apps to offer one button per entry.
pub async fn list(State(server): State<Arc<Server>>) -> Json<Vec<LauncherInfo>> {
    Json(launcher::entries(&server.config()))
}

/// `POST /api/launcher/<name>`: spawn that entry on the desktop and answer with its pid.
/// The answer is the report back; what the launched process writes lands in watched
/// folders and lights the watchers, which is how the rest of the loop closes. An entry
/// whose folder is outside the allowed scope refuses to launch (403); one that will not
/// start answers 500 with the OS error.
pub async fn launch(
    State(server): State<Arc<Server>>,
    Path(name): Path<String>,
) -> Result<Json<LaunchResult>, ApiError> {
    let config = server.config();
    let Some(entry) = launcher::entry(&config, &name) else {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            format!("no [[launcher]] entry named {name} in config.toml"),
        ));
    };
    let folder =
        launcher::scoped_folder(&config, entry).map_err(|e| ApiError(StatusCode::FORBIDDEN, e))?;
    let result = launcher::spawn(entry, &folder)
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(result))
}
