mod config;
mod error;
pub mod repositories;

pub use config::PostgresConfig;
pub use error::StorageError;

use sea_orm::{ConnectionTrait, Database, DatabaseConnection, Statement};

/// Storage boundary for database connections.
#[derive(Debug, Clone)]
pub struct Storage {
    connection: DatabaseConnection,
}

impl Storage {
    /// Creates a storage instance by connecting to `PostgreSQL`.
    ///
    /// # Errors
    ///
    /// Returns an error when the connection to `PostgreSQL` fails.
    pub async fn connect(config: &PostgresConfig) -> Result<Self, StorageError> {
        let connection = Database::connect(config.connect_options())
            .await
            .map_err(StorageError::Connect)?;
        Ok(Self { connection })
    }

    /// Returns the shared `SeaORM` database connection.
    #[must_use]
    pub fn connection(&self) -> &DatabaseConnection {
        &self.connection
    }

    /// Runs a lightweight query to verify database reachability.
    ///
    /// # Errors
    ///
    /// Returns an error when the database ping query fails.
    pub async fn ping(&self) -> Result<(), StorageError> {
        self.connection
            .execute(Statement::from_string(
                sea_orm::DatabaseBackend::Postgres,
                "select 1".to_owned(),
            ))
            .await
            .map(|_| ())
            .map_err(StorageError::Ping)
    }
}
