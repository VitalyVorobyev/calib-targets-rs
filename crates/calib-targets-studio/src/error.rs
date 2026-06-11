//! JSON API error type: every handler error becomes `{ "error": "..." }`
//! with an appropriate HTTP status code.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// API-level error carrying the HTTP status it should map to.
#[derive(Debug)]
pub enum ApiError {
    /// 404 — unknown label, missing baseline, absent private file.
    NotFound(String),
    /// 400 — malformed label, invalid params JSON, bad request body.
    BadRequest(String),
    /// 500 — I/O or internal failure.
    Internal(String),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> &str {
        match self {
            ApiError::NotFound(m) | ApiError::BadRequest(m) | ApiError::Internal(m) => m,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status(), Json(json!({ "error": self.message() }))).into_response()
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::Internal(e.to_string())
    }
}
