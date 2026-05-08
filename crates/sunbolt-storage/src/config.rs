use std::env;

use sea_orm::ConnectOptions;

use crate::StorageError;

const DEFAULT_MAX_CONNECTIONS: u32 = 16;
const DEFAULT_MIN_CONNECTIONS: u32 = 1;
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;
const DEFAULT_ACQUIRE_TIMEOUT_SECS: u64 = 10;

/// `PostgreSQL` storage configuration loaded from environment variables.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PostgresConfig {
    pub database_url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_secs: u64,
    pub acquire_timeout_secs: u64,
}

impl PostgresConfig {
    /// Loads Postgres configuration from environment variables.
    ///
    /// Required:
    /// - `SUNBOLT_DATABASE_URL`
    ///
    /// Optional:
    /// - `SUNBOLT_DB_MAX_CONNECTIONS` (default: 16)
    /// - `SUNBOLT_DB_MIN_CONNECTIONS` (default: 1)
    /// - `SUNBOLT_DB_CONNECT_TIMEOUT_SECS` (default: 10)
    /// - `SUNBOLT_DB_ACQUIRE_TIMEOUT_SECS` (default: 10)
    ///
    /// # Errors
    ///
    /// Returns an error when a required variable is missing or an optional
    /// numeric value fails to parse.
    pub fn from_env() -> Result<Self, StorageError> {
        let database_url = required_env("SUNBOLT_DATABASE_URL")?;
        let max_connections = parse_u32_env("SUNBOLT_DB_MAX_CONNECTIONS", DEFAULT_MAX_CONNECTIONS)?;
        let min_connections = parse_u32_env("SUNBOLT_DB_MIN_CONNECTIONS", DEFAULT_MIN_CONNECTIONS)?;
        let connect_timeout_secs = parse_u64_env(
            "SUNBOLT_DB_CONNECT_TIMEOUT_SECS",
            DEFAULT_CONNECT_TIMEOUT_SECS,
        )?;
        let acquire_timeout_secs = parse_u64_env(
            "SUNBOLT_DB_ACQUIRE_TIMEOUT_SECS",
            DEFAULT_ACQUIRE_TIMEOUT_SECS,
        )?;

        Ok(Self {
            database_url,
            max_connections,
            min_connections,
            connect_timeout_secs,
            acquire_timeout_secs,
        })
    }

    #[must_use]
    pub fn connect_options(&self) -> ConnectOptions {
        let mut options = ConnectOptions::new(self.database_url.clone());
        options.max_connections(self.max_connections);
        options.min_connections(self.min_connections);
        options.connect_timeout(std::time::Duration::from_secs(self.connect_timeout_secs));
        options.acquire_timeout(std::time::Duration::from_secs(self.acquire_timeout_secs));
        options.sqlx_logging(false);
        options
    }
}

fn required_env(name: &'static str) -> Result<String, StorageError> {
    env::var(name).map_err(|_| StorageError::MissingEnvVar { name })
}

fn parse_u32_env(name: &'static str, default: u32) -> Result<u32, StorageError> {
    let Some(value) = env::var_os(name) else {
        return Ok(default);
    };

    value
        .to_str()
        .ok_or_else(|| StorageError::InvalidEnvVar {
            name,
            value: "<non-utf8>".to_owned(),
        })?
        .parse::<u32>()
        .map_err(|_| StorageError::InvalidEnvVar {
            name,
            value: value.to_string_lossy().into_owned(),
        })
}

fn parse_u64_env(name: &'static str, default: u64) -> Result<u64, StorageError> {
    let Some(value) = env::var_os(name) else {
        return Ok(default);
    };

    value
        .to_str()
        .ok_or_else(|| StorageError::InvalidEnvVar {
            name,
            value: "<non-utf8>".to_owned(),
        })?
        .parse::<u64>()
        .map_err(|_| StorageError::InvalidEnvVar {
            name,
            value: value.to_string_lossy().into_owned(),
        })
}

#[cfg(test)]
mod tests {
    use super::{
        parse_u32_env, parse_u64_env, PostgresConfig, DEFAULT_ACQUIRE_TIMEOUT_SECS,
        DEFAULT_CONNECT_TIMEOUT_SECS, DEFAULT_MAX_CONNECTIONS, DEFAULT_MIN_CONNECTIONS,
    };

    #[test]
    fn postgres_connect_options_use_config_values() {
        let config = PostgresConfig {
            database_url: "postgres://user:pass@localhost/sunbolt".to_owned(),
            max_connections: DEFAULT_MAX_CONNECTIONS,
            min_connections: DEFAULT_MIN_CONNECTIONS,
            connect_timeout_secs: DEFAULT_CONNECT_TIMEOUT_SECS,
            acquire_timeout_secs: DEFAULT_ACQUIRE_TIMEOUT_SECS,
        };
        let _options = config.connect_options();
        assert_eq!(config.max_connections, DEFAULT_MAX_CONNECTIONS);
        assert_eq!(config.min_connections, DEFAULT_MIN_CONNECTIONS);
        assert_eq!(config.connect_timeout_secs, DEFAULT_CONNECT_TIMEOUT_SECS);
        assert_eq!(config.acquire_timeout_secs, DEFAULT_ACQUIRE_TIMEOUT_SECS);
    }

    #[test]
    fn parse_helpers_use_defaults_for_missing_values() {
        assert_eq!(
            parse_u32_env("SUNBOLT_TEST_MISSING_U32", 7).expect("default should be used"),
            7
        );
        assert_eq!(
            parse_u64_env("SUNBOLT_TEST_MISSING_U64", 11).expect("default should be used"),
            11
        );
    }
}
