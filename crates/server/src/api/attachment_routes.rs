//! Attachments: uploaded before the message that refers to them, and served back to clients.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Multipart, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use composer_protocol::Attachment;

use crate::api::ApiError;
use crate::attachments::{resolve, save};
use crate::server::Server;

/// How many files one upload request can carry (each up to `[attachments] max_mb`).
pub const FILES_PER_REQUEST: u64 = 5;

/// `POST /api/sessions/{id}/attachments`, multipart: every file part is checked and stored;
/// the reply lists them, for `message.send`'s `attachments`.
pub async fn upload(
    State(server): State<Arc<Server>>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<Vec<Attachment>>, ApiError> {
    server
        .sessions
        .get(&id)
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, format!("no session {id}")))?;
    let mut saved = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let Some(name) = field.file_name().map(str::to_string) else {
            continue;
        };
        if saved.len() as u64 >= FILES_PER_REQUEST {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                format!("at most {FILES_PER_REQUEST} files per upload"),
            ));
        }
        let bytes = field
            .bytes()
            .await
            .map_err(|e| ApiError(StatusCode::PAYLOAD_TOO_LARGE, e.to_string()))?;
        let (server, id) = (server.clone(), id.clone());
        let attachment = tokio::task::spawn_blocking(move || {
            save(
                &server.home,
                &server.config().attachments,
                &id,
                &name,
                &bytes,
            )
        })
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
        saved.push(attachment);
    }
    if saved.is_empty() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "no file in the upload".into(),
        ));
    }
    Ok(Json(saved))
}

/// `GET /api/sessions/{id}/attachments/{file}`: the stored file (a thumbnail, a download).
pub async fn get(
    State(server): State<Arc<Server>>,
    Path((id, file)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let probe = Attachment {
        file,
        name: String::new(),
        mime: String::new(),
        size: 0,
    };
    let (attachment, path) =
        resolve(&server.home, &id, &probe).map_err(|e| ApiError(StatusCode::NOT_FOUND, e))?;
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((
        [
            (header::CONTENT_TYPE, attachment.mime),
            // Never rendered as a page of this origin.
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", attachment.name),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
        ],
        bytes,
    )
        .into_response())
}
