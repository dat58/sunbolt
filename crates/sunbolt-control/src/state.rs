use serde::Serialize;
use sunbolt_audit::AuditLog;
use sunbolt_auth::{AuthConfig, AuthService};
use sunbolt_storage::{PostgresConfig, Storage, StorageError};

use crate::{
    agent::AgentConnectionRegistry,
    config::{
        allowed_origins_from_env, ProductionStateConfig, RuntimeMode, TerminalSessionConfig,
        DEFAULT_LOGIN_RATE_MAX, DEFAULT_LOGIN_RATE_WINDOW, DEFAULT_TERMINAL_RATE_MAX,
        DEFAULT_TERMINAL_RATE_WINDOW,
    },
    error::StartupError,
    node::NodeEnrollmentRegistry,
    rate_limit::SlidingWindowRateLimiter,
    routing::InMemoryNodeRouter,
    terminal::TerminalSessionRegistry,
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) runtime_mode: RuntimeMode,
    pub(crate) production_state: ProductionState,
    pub(crate) sessions: TerminalSessionRegistry,
    pub(crate) terminal_config: TerminalSessionConfig,
    pub(crate) auth: AuthService,
    pub(crate) audit: AuditLog,
    pub(crate) node_enrollment: NodeEnrollmentRegistry,
    pub(crate) agent_connections: AgentConnectionRegistry,
    pub(crate) node_router: InMemoryNodeRouter,
    pub(crate) login_rate_limiter: SlidingWindowRateLimiter,
    pub(crate) terminal_rate_limiter: SlidingWindowRateLimiter,
    pub(crate) allowed_origins: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct ProductionState {
    pub(crate) storage: Option<Storage>,
    pub(crate) backends: ProductionStateConfig,
}

impl AppState {
    pub(crate) async fn try_from_env() -> Result<Self, StartupError> {
        let runtime_mode = RuntimeMode::from_env()?;
        let production_state = ProductionState::from_mode(runtime_mode).await?;
        let auth_config = auth_config_for_mode(runtime_mode);

        Ok(Self::new(
            runtime_mode,
            production_state,
            AuthService::new(auth_config),
        ))
    }

    pub(crate) fn development_from_env() -> Self {
        Self::new(
            RuntimeMode::Development,
            ProductionState::development(),
            AuthService::from_env(),
        )
    }

    #[cfg(test)]
    pub(crate) fn development_with_auth(auth: AuthService) -> Self {
        Self::new(
            RuntimeMode::Development,
            ProductionState::development(),
            auth,
        )
    }

    fn new(
        runtime_mode: RuntimeMode,
        production_state: ProductionState,
        auth: AuthService,
    ) -> Self {
        Self {
            runtime_mode,
            production_state,
            sessions: TerminalSessionRegistry::default(),
            terminal_config: TerminalSessionConfig::from_env(),
            auth,
            audit: AuditLog::default(),
            node_enrollment: NodeEnrollmentRegistry::default(),
            agent_connections: AgentConnectionRegistry::default(),
            node_router: InMemoryNodeRouter::default(),
            login_rate_limiter: SlidingWindowRateLimiter::new(
                DEFAULT_LOGIN_RATE_WINDOW,
                DEFAULT_LOGIN_RATE_MAX,
            ),
            terminal_rate_limiter: SlidingWindowRateLimiter::new(
                DEFAULT_TERMINAL_RATE_WINDOW,
                DEFAULT_TERMINAL_RATE_MAX,
            ),
            allowed_origins: allowed_origins_from_env(),
        }
    }

    pub(crate) fn state_summary(&self) -> StateSummary {
        StateSummary {
            runtime_mode: self.runtime_mode.as_str(),
            users: self.production_state.backends.users,
            auth_sessions: self.production_state.backends.auth_sessions,
            recent_mfa: self.production_state.backends.recent_mfa,
            nodes_and_credentials: self.production_state.backends.nodes_and_credentials,
            terminal_session_metadata: self.production_state.backends.terminal_session_metadata,
            audit_logs: self.production_state.backends.audit_logs,
            postgres_connected: self.production_state.storage.is_some(),
        }
    }
}

impl ProductionState {
    async fn from_mode(mode: RuntimeMode) -> Result<Self, StartupError> {
        if !mode.is_production() {
            return Ok(Self::development());
        }

        let backends = ProductionStateConfig::postgres();
        backends.validate_for(mode)?;
        let postgres_config = required_postgres_config_for_mode(mode, PostgresConfig::from_env)?
            .expect("production mode requires postgres config");
        let storage = Storage::connect(&postgres_config)
            .await
            .map_err(StartupError::StorageUnavailable)?;
        storage
            .ping()
            .await
            .map_err(StartupError::StorageUnavailable)?;

        Ok(Self {
            storage: Some(storage),
            backends,
        })
    }

    fn development() -> Self {
        Self {
            storage: None,
            backends: ProductionStateConfig::development_runtime_only(),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub(crate) struct StateSummary {
    pub(crate) runtime_mode: &'static str,
    pub(crate) users: crate::config::ProductionStateBackend,
    pub(crate) auth_sessions: crate::config::ProductionStateBackend,
    pub(crate) recent_mfa: crate::config::ProductionStateBackend,
    pub(crate) nodes_and_credentials: crate::config::ProductionStateBackend,
    pub(crate) terminal_session_metadata: crate::config::ProductionStateBackend,
    pub(crate) audit_logs: crate::config::ProductionStateBackend,
    pub(crate) postgres_connected: bool,
}

fn auth_config_for_mode(mode: RuntimeMode) -> AuthConfig {
    let mut config = AuthConfig::from_env();
    if mode.is_production() {
        config.bootstrap_admin = false;
        config.secure_cookie = true;
    }
    config
}

fn required_postgres_config_for_mode(
    mode: RuntimeMode,
    load: impl FnOnce() -> Result<PostgresConfig, StorageError>,
) -> Result<Option<PostgresConfig>, StartupError> {
    if mode.is_production() {
        load()
            .map(Some)
            .map_err(StartupError::MissingProductionStorage)
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        auth_config_for_mode, required_postgres_config_for_mode, ProductionState, RuntimeMode,
    };
    use sunbolt_auth::AuthService;
    use sunbolt_storage::{PostgresConfig, StorageError};

    #[test]
    fn production_auth_config_disables_development_bootstrap_admin() {
        let config = auth_config_for_mode(RuntimeMode::Production);

        assert!(!config.bootstrap_admin);
        assert!(config.secure_cookie);
    }

    #[test]
    fn production_auth_service_does_not_accept_development_bootstrap_admin() {
        let auth = AuthService::new(auth_config_for_mode(RuntimeMode::Production));

        assert!(auth
            .login("admin@sunbolt.local", "sunbolt-dev-admin")
            .is_err());
    }

    #[test]
    fn development_state_summary_keeps_runtime_handles_out_of_durable_state() {
        let state = ProductionState::development();

        assert!(state.storage.is_none());
        assert!(!state.backends.users.is_durable_or_recoverable());
        assert!(!state
            .backends
            .terminal_session_metadata
            .is_durable_or_recoverable());
    }

    #[test]
    fn production_requires_postgres_storage_config() {
        let error = required_postgres_config_for_mode(RuntimeMode::Production, || {
            Err(StorageError::MissingEnvVar {
                name: "SUNBOLT_DATABASE_URL",
            })
        })
        .expect_err("production should require database config");

        assert!(matches!(
            error,
            crate::error::StartupError::MissingProductionStorage(_)
        ));
    }

    #[test]
    fn development_does_not_require_postgres_storage_config() {
        let config = required_postgres_config_for_mode(RuntimeMode::Development, || {
            Ok(PostgresConfig {
                database_url: "postgres://unused".to_owned(),
                max_connections: 1,
                min_connections: 1,
                connect_timeout_secs: 1,
                acquire_timeout_secs: 1,
            })
        })
        .expect("development should not require database config");

        assert!(config.is_none());
    }
}
