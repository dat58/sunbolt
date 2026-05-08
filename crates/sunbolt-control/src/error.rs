use axum::http::StatusCode;
use serde::Serialize;
use sunbolt_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Serialize)]
pub(crate) struct ErrorResponse {
    pub(crate) error: &'static str,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum TerminalAuthorizationError {
    Unauthorized,
    Forbidden,
    StepUpMfaRequired,
    Internal,
}

impl TerminalAuthorizationError {
    pub(crate) const fn status_code(self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden | Self::StepUpMfaRequired => StatusCode::FORBIDDEN,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::Unauthorized => "terminal websocket authorization failed",
            Self::Forbidden => "terminal access is forbidden",
            Self::StepUpMfaRequired => "step-up MFA is required before opening a terminal",
            Self::Internal => "terminal authorization service unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum SessionLimitError {
    GlobalCapacity,
    PerUser,
    PerNode,
}

impl SessionLimitError {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::GlobalCapacity => "maximum terminal session count reached",
            Self::PerUser => "maximum terminal sessions per user reached",
            Self::PerNode => "maximum terminal sessions per node reached",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum EnrollmentError {
    InvalidToken,
    TokenUsed,
    TokenExpired,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum NodeConnectionError {
    UnknownNode,
    InvalidCredential,
    Revoked,
}

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("{0}")]
    InvalidRuntimeMode(String),
    #[error("production startup requires durable storage: {0}")]
    MissingProductionStorage(#[source] StorageError),
    #[error("production storage is not reachable: {0}")]
    StorageUnavailable(#[source] StorageError),
    #[error("production startup requires {state} to use durable or recoverable storage")]
    NonDurableProductionState { state: &'static str },
}
