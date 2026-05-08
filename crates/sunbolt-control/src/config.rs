use std::{env, str::FromStr, time::Duration};

use serde::Serialize;

use crate::error::StartupError;

const DEFAULT_MAX_TERMINAL_SESSIONS: usize = 16;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const DEFAULT_DISCONNECT_GRACE: Duration = Duration::from_secs(30);
pub(crate) const NODE_OFFLINE_AFTER: Duration = Duration::from_secs(90);
pub(crate) const DEFAULT_MAX_SESSIONS_PER_USER: usize = 5;
pub(crate) const DEFAULT_MAX_SESSIONS_PER_NODE: usize = 10;
pub(crate) const DEFAULT_MAX_DURATION: Duration = Duration::from_secs(8 * 60 * 60);
pub(crate) const DEFAULT_LOGIN_RATE_WINDOW: Duration = Duration::from_secs(15 * 60);
pub(crate) const DEFAULT_LOGIN_RATE_MAX: usize = 10;
pub(crate) const DEFAULT_TERMINAL_RATE_WINDOW: Duration = Duration::from_secs(60);
pub(crate) const DEFAULT_TERMINAL_RATE_MAX: usize = 5;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum RuntimeMode {
    Development,
    Production,
}

impl RuntimeMode {
    pub(crate) fn from_env() -> Result<Self, StartupError> {
        env::var("SUNBOLT_ENV")
            .unwrap_or_else(|_| "development".to_owned())
            .parse()
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }

    pub(crate) const fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

impl FromStr for RuntimeMode {
    type Err = StartupError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "" | "development" => Ok(Self::Development),
            "production" => Ok(Self::Production),
            other => Err(StartupError::InvalidRuntimeMode(format!(
                "invalid SUNBOLT_ENV `{other}`; expected `development` or `production`"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProductionStateBackend {
    Postgres,
    RecoverableFromDurableSessionStore,
    RuntimeOnly,
}

impl ProductionStateBackend {
    pub(crate) const fn is_durable_or_recoverable(self) -> bool {
        matches!(
            self,
            Self::Postgres | Self::RecoverableFromDurableSessionStore
        )
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct ProductionStateConfig {
    pub(crate) users: ProductionStateBackend,
    pub(crate) auth_sessions: ProductionStateBackend,
    pub(crate) recent_mfa: ProductionStateBackend,
    pub(crate) nodes_and_credentials: ProductionStateBackend,
    pub(crate) terminal_session_metadata: ProductionStateBackend,
    pub(crate) audit_logs: ProductionStateBackend,
}

impl ProductionStateConfig {
    pub(crate) const fn postgres() -> Self {
        Self {
            users: ProductionStateBackend::Postgres,
            auth_sessions: ProductionStateBackend::Postgres,
            recent_mfa: ProductionStateBackend::RecoverableFromDurableSessionStore,
            nodes_and_credentials: ProductionStateBackend::Postgres,
            terminal_session_metadata: ProductionStateBackend::Postgres,
            audit_logs: ProductionStateBackend::Postgres,
        }
    }

    pub(crate) const fn development_runtime_only() -> Self {
        Self {
            users: ProductionStateBackend::RuntimeOnly,
            auth_sessions: ProductionStateBackend::RuntimeOnly,
            recent_mfa: ProductionStateBackend::RuntimeOnly,
            nodes_and_credentials: ProductionStateBackend::RuntimeOnly,
            terminal_session_metadata: ProductionStateBackend::RuntimeOnly,
            audit_logs: ProductionStateBackend::RuntimeOnly,
        }
    }

    pub(crate) fn validate_for(self, mode: RuntimeMode) -> Result<(), StartupError> {
        if !mode.is_production() {
            return Ok(());
        }

        for (state, backend) in self.required_states() {
            if !backend.is_durable_or_recoverable() {
                return Err(StartupError::NonDurableProductionState { state });
            }
        }
        Ok(())
    }

    fn required_states(self) -> [(&'static str, ProductionStateBackend); 6] {
        [
            ("users", self.users),
            ("auth sessions", self.auth_sessions),
            ("recent MFA state", self.recent_mfa),
            ("nodes and credentials", self.nodes_and_credentials),
            ("terminal session metadata", self.terminal_session_metadata),
            ("audit logs", self.audit_logs),
        ]
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct TerminalSessionConfig {
    pub(crate) max_sessions: usize,
    pub(crate) max_sessions_per_user: usize,
    pub(crate) max_sessions_per_node: usize,
    pub(crate) idle_timeout: Duration,
    pub(crate) max_duration: Duration,
    pub(crate) disconnect_grace: Duration,
}

impl TerminalSessionConfig {
    pub(crate) fn from_env() -> Self {
        Self {
            max_sessions: env_usize("SUNBOLT_MAX_TERMINAL_SESSIONS")
                .unwrap_or(DEFAULT_MAX_TERMINAL_SESSIONS),
            max_sessions_per_user: env_usize("SUNBOLT_MAX_TERMINAL_SESSIONS_PER_USER")
                .unwrap_or(DEFAULT_MAX_SESSIONS_PER_USER),
            max_sessions_per_node: env_usize("SUNBOLT_MAX_TERMINAL_SESSIONS_PER_NODE")
                .unwrap_or(DEFAULT_MAX_SESSIONS_PER_NODE),
            idle_timeout: env_duration_secs("SUNBOLT_TERMINAL_IDLE_TIMEOUT_SECS")
                .unwrap_or(DEFAULT_IDLE_TIMEOUT),
            max_duration: env_duration_secs("SUNBOLT_TERMINAL_MAX_DURATION_SECS")
                .unwrap_or(DEFAULT_MAX_DURATION),
            disconnect_grace: env_duration_secs("SUNBOLT_TERMINAL_DISCONNECT_GRACE_SECS")
                .unwrap_or(DEFAULT_DISCONNECT_GRACE),
        }
    }
}

pub(crate) fn allowed_origins_from_env() -> Vec<String> {
    env::var("SUNBOLT_ALLOWED_ORIGINS")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.split(',').map(|o| o.trim().to_owned()).collect())
        .unwrap_or_default()
}

fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.parse().ok()
}

fn env_duration_secs(name: &str) -> Option<Duration> {
    env_usize(name).and_then(|seconds| u64::try_from(seconds).ok().map(Duration::from_secs))
}

#[cfg(test)]
mod tests {
    use super::{ProductionStateBackend, ProductionStateConfig, RuntimeMode};

    #[test]
    fn runtime_mode_accepts_only_development_and_production() {
        assert_eq!(
            "development".parse::<RuntimeMode>().expect("valid mode"),
            RuntimeMode::Development
        );
        assert_eq!(
            "production".parse::<RuntimeMode>().expect("valid mode"),
            RuntimeMode::Production
        );
        assert!("preview".parse::<RuntimeMode>().is_err());
    }

    #[test]
    fn production_state_requires_durable_or_recoverable_backends() {
        ProductionStateConfig::postgres()
            .validate_for(RuntimeMode::Production)
            .expect("postgres-backed production state is valid");

        let invalid = ProductionStateConfig {
            users: ProductionStateBackend::RuntimeOnly,
            ..ProductionStateConfig::postgres()
        };

        assert!(invalid.validate_for(RuntimeMode::Production).is_err());
        invalid
            .validate_for(RuntimeMode::Development)
            .expect("development may use runtime-only scaffolding");
    }
}
