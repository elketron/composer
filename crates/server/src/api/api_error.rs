use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use composer_core::CoreError;
use serde_json::json;

use crate::error::ServerError;

/// An HTTP error with a JSON body `{ "error": "..." }`.
#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<ServerError> for ApiError {
    fn from(error: ServerError) -> Self {
        let status = match &error {
            ServerError::Core(CoreError::PageNotFound(_) | CoreError::SiteNotFound(_))
            | ServerError::SessionNotFound(_)
            | ServerError::AgentNotFound(_) => StatusCode::NOT_FOUND,
            ServerError::Core(CoreError::InvalidUrl { .. }) | ServerError::Invalid(_) => {
                StatusCode::BAD_REQUEST
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self(status, error.to_string())
    }
}

impl From<CoreError> for ApiError {
    fn from(error: CoreError) -> Self {
        ServerError::Core(error).into()
    }
}
