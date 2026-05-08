use sea_orm::DbErr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("missing required environment variable `{name}`")]
    MissingEnvVar { name: &'static str },
    #[error("invalid value for `{name}`: {value}")]
    InvalidEnvVar { name: &'static str, value: String },
    #[error("failed to connect to postgres: {0}")]
    Connect(#[source] DbErr),
    #[error("failed to ping postgres: {0}")]
    Ping(#[source] DbErr),
    #[error("repository `{boundary}` failed during `{operation}`: {source}")]
    Repository {
        boundary: &'static str,
        operation: &'static str,
        #[source]
        source: DbErr,
    },
}
