use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use serde::Serialize;
use thiserror::Error;

/// Unified application error type mapped to HTTP status codes.
#[derive(Debug, Error)]
pub enum AppError {
    /// Server misconfiguration (HTTP 500).
    #[error("configuration error: {0}")]
    Config(String),
    /// Client-supplied data failed validation (HTTP 400).
    #[error("validation error: {0}")]
    Validation(String),
    /// Requested resource does not exist (HTTP 404).
    #[error("not found: {0}")]
    NotFound(String),
    /// Write would violate a uniqueness or integrity constraint (HTTP 409).
    #[error("conflict: {0}")]
    Conflict(String),
    /// Caller lacks permission for the requested action (HTTP 403).
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// Caller has not authenticated or provided a valid token (HTTP 401).
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    /// Upstream authorization service returned an error (HTTP 502).
    #[error("authorization service error: {0}")]
    Authz(String),
    /// A required backend service is temporarily unavailable (HTTP 503).
    #[error("service unavailable: {0}")]
    Unavailable(String),
    /// Unexpected internal error (HTTP 500).
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// Create a configuration error.
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }

    /// Create a validation error.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    /// Create a not-found error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    /// Create a conflict error.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    /// Create a forbidden error.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    /// Create an unauthorized error.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }

    /// Create an authorization-service error.
    pub fn authz(message: impl Into<String>) -> Self {
        Self::Authz(message.into())
    }

    /// Create a service-unavailable error.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::Unavailable(message.into())
    }

    /// Create an internal error from any displayable value.
    pub fn internal(error: impl std::fmt::Display) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<diesel::result::Error> for AppError {
    fn from(value: diesel::result::Error) -> Self {
        Self::internal(value)
    }
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    message: String,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Authz(_) => StatusCode::BAD_GATEWAY,
            AppError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let kind = match self {
            AppError::Config(_) => "config_error",
            AppError::Validation(_) => "validation_error",
            AppError::NotFound(_) => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::Forbidden(_) => "forbidden",
            AppError::Unauthorized(_) => "unauthorized",
            AppError::Authz(_) => "authz_error",
            AppError::Unavailable(_) => "service_unavailable",
            AppError::Internal(_) => "internal_error",
        };

        let status = self.status_code();
        if status.is_server_error() {
            tracing::error!(error_kind = kind, %status, error = %self, "server error");
        } else if status.is_client_error() {
            tracing::warn!(error_kind = kind, %status, error = %self, "client error");
        }

        HttpResponse::build(status).json(ErrorBody {
            error: kind,
            message: self.to_string(),
        })
    }
}
