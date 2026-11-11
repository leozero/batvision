use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

/// Error body returned by every endpoint. `code` is stable and meant for clients;
/// `message` is for humans and logs.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{1}")]
    BadRequest(&'static str, String),
    #[error("authentication required")]
    Unauthenticated,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("{1}")]
    Forbidden(&'static str, String),
    #[error("not found")]
    NotFound,
    #[error("{1}")]
    Conflict(&'static str, String),
    #[error("too many attempts, retry in {retry_after_secs}s")]
    TooManyAttempts { retry_after_secs: u64 },
    #[error("upstream service error: {0}")]
    Upstream(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    pub fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::BadRequest(code, message.into())
    }
    pub fn forbidden(code: &'static str, message: impl Into<String>) -> Self {
        Self::Forbidden(code, message.into())
    }
    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::Conflict(code, message.into())
    }

    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::BadRequest(code, _) => (StatusCode::BAD_REQUEST, code),
            Self::Unauthenticated => (StatusCode::UNAUTHORIZED, "unauthenticated"),
            Self::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            Self::Forbidden(code, _) => (StatusCode::FORBIDDEN, code),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict(code, _) => (StatusCode::CONFLICT, code),
            Self::TooManyAttempts { .. } => (StatusCode::TOO_MANY_REQUESTS, "too_many_attempts"),
            Self::Upstream(_) => (StatusCode::BAD_GATEWAY, "upstream_error"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound,
            other => Self::Internal(other.into()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        let message = match &self {
            // Never leak internals to clients.
            Self::Internal(err) => {
                tracing::error!(error = ?err, "internal error");
                "internal server error".to_owned()
            }
            other => other.to_string(),
        };
        let mut res = (status, Json(ErrorBody { code, message })).into_response();
        if let Self::TooManyAttempts { retry_after_secs } = self
            && let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string())
        {
            res.headers_mut().insert(header::RETRY_AFTER, value);
        }
        res
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
