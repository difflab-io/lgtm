//! Errors surfaced over the Connect-JSON API. These map 1:1 to the Connect
//! error-code enum so clients can distinguish user error from server error.

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("git error: {0}")]
    Git(#[from] git2::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl ServiceError {
    pub fn code(&self) -> ConnectCode {
        match self {
            Self::InvalidArgument(_) => ConnectCode::InvalidArgument,
            Self::NotFound(_) => ConnectCode::NotFound,
            Self::Git(_) | Self::Other(_) => ConnectCode::Internal,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ConnectCode {
    InvalidArgument,
    NotFound,
    Internal,
}

impl ConnectCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidArgument => "invalid_argument",
            Self::NotFound => "not_found",
            Self::Internal => "internal",
        }
    }

    pub fn http_status(self) -> u16 {
        // Mapping follows the Connect protocol spec.
        match self {
            Self::InvalidArgument => 400,
            Self::NotFound => 404,
            Self::Internal => 500,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ConnectErrorBody {
    pub code: &'static str,
    pub message: String,
}
