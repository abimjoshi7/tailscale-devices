use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde::Serialize;
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(&'static str),

    #[error("bad request: {message}")]
    BadRequest {
        code: &'static str,
        message: String,
    },

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden: {message}")]
    Forbidden {
        code: &'static str,
        message: String,
    },

    #[error("upstream failure: {0:#}")]
    Upstream(#[source] anyhow::Error),

    #[error("internal error: {0:#}")]
    Internal(#[source] anyhow::Error),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "not_found",
            AppError::BadRequest { code, .. } => code,
            AppError::Unauthorized => "unauthorized",
            AppError::Forbidden { code, .. } => code,
            AppError::Upstream(_) => "upstream_failure",
            AppError::Internal(_) => "internal_error",
        }
    }

    pub fn public_message(&self) -> String {
        match self {
            AppError::NotFound(what) => format!("{} not found", what),
            AppError::BadRequest { message, .. } => message.clone(),
            AppError::Unauthorized => "missing or invalid API key".to_string(),
            AppError::Forbidden { message, .. } => message.clone(),
            AppError::Upstream(_) => "upstream service unavailable".to_string(),
            AppError::Internal(_) => "internal server error".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::BadRequest { .. } => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden { .. } => StatusCode::FORBIDDEN,
            AppError::Upstream(_) => StatusCode::BAD_GATEWAY,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::Upstream(e) => tracing::error!(error = ?e, "upstream failure"),
            AppError::Internal(e) => tracing::error!(error = ?e, "internal error"),
            _ => {}
        }

        let body = ErrorBody {
            error: self.code(),
            message: self.public_message(),
        };

        let json = serde_json::to_string_pretty(&body)
            .unwrap_or_else(|_| r#"{"error":"internal_error","message":"serialization failure"}"#.to_string());

        HttpResponse::build(self.status_code())
            .content_type("application/json")
            .body(json)
    }
}

#[derive(Debug)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "configuration error: {}", self.0)
    }
}

impl std::error::Error for ConfigError {}
