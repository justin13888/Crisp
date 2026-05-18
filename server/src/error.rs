//! The API error type and its HTTP projection.
//!
//! Every handler returns [`ApiResult`]; an [`ApiError`] is rendered as a JSON
//! body `{ "error": <code>, "message": <text> }` with a matching status. The
//! verification failure variant additionally carries the full compliance
//! report so a publishing client can see exactly which check failed.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use crisp_core::verify::ComplianceReport;
use serde_json::json;

/// The result type returned by every API handler.
pub type ApiResult<T> = Result<T, ApiError>;

/// A failure surfaced to an API caller.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The requested resource does not exist.
    #[error("not found")]
    NotFound,
    /// The request was malformed or semantically invalid.
    #[error("{0}")]
    BadRequest(String),
    /// Authentication is missing or invalid.
    #[error("unauthorized")]
    Unauthorized,
    /// The request conflicts with existing state (e.g. a duplicate).
    #[error("{0}")]
    Conflict(String),
    /// A submitted bundle failed the A–E verification pipeline.
    #[error("bundle failed verification")]
    Verification(Box<ComplianceReport>),
    /// The client and server disagree on a supported version.
    #[error("{0}")]
    Version(String),
    /// An unexpected internal failure; details are logged, not exposed.
    #[error("internal server error")]
    Internal,
}

impl ApiError {
    /// A short, stable, machine-readable code for this error.
    fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::Unauthorized => "unauthorized",
            Self::Conflict(_) => "conflict",
            Self::Verification(_) => "verification_failed",
            Self::Version(_) => "version_unsupported",
            Self::Internal => "internal",
        }
    }

    /// The HTTP status this error maps to.
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Verification(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Version(_) => StatusCode::CONFLICT,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let mut body = json!({ "error": self.code(), "message": self.to_string() });
        if let Self::Verification(report) = &self {
            body["report"] = serde_json::to_value(report).unwrap_or(serde_json::Value::Null);
        }
        (status, Json(body)).into_response()
    }
}

impl From<sea_orm::DbErr> for ApiError {
    fn from(error: sea_orm::DbErr) -> Self {
        tracing::error!(%error, "database error");
        Self::Internal
    }
}

impl From<crisp_core::CrispError> for ApiError {
    fn from(error: crisp_core::CrispError) -> Self {
        Self::BadRequest(error.to_string())
    }
}
