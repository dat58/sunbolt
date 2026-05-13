use std::{env, str::FromStr, time::Duration};

use serde::Serialize;
use sunbolt_auth::AuthConfig;

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
pub(crate) const DEFAULT_MFA_RATE_WINDOW: Duration = Duration::from_secs(5 * 60);
pub(crate) const DEFAULT_MFA_RATE_MAX: usize = 5;
pub(crate) const DEFAULT_TERMINAL_RATE_WINDOW: Duration = Duration::from_secs(60);
pub(crate) const DEFAULT_TERMINAL_RATE_MAX: usize = 5;
pub(crate) const DEFAULT_ENROLLMENT_TOKEN_RATE_WINDOW: Duration = Duration::from_secs(15 * 60);
pub(crate) const DEFAULT_ENROLLMENT_TOKEN_RATE_MAX: usize = 5;
pub(crate) const DEFAULT_AGENT_AUTH_FAILURE_RATE_WINDOW: Duration = Duration::from_secs(5 * 60);
pub(crate) const DEFAULT_AGENT_AUTH_FAILURE_RATE_MAX: usize = 10;
const REQUIRED_PRODUCTION_CONFIG_VARS: [&str; 2] = ["SUNBOLT_DATABASE_URL", "SUNBOLT_PUBLIC_URL"];

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum RuntimeMode {
    Development,
    Production,
}

impl RuntimeMode {
    pub(crate) fn from_env() -> Result<Self, StartupError> {
        match env::var("SUNBOLT_ENV") {
            Ok(value) => Self::from_env_value(Some(value)),
            Err(env::VarError::NotPresent) => Self::from_env_value(None),
            Err(env::VarError::NotUnicode(_)) => Err(StartupError::InvalidRuntimeMode(
                "invalid SUNBOLT_ENV; value must be valid Unicode".to_owned(),
            )),
        }
    }

    fn from_env_value(value: Option<String>) -> Result<Self, StartupError> {
        let Some(value) = value else {
            return Err(StartupError::MissingRequiredEnv {
                name: "SUNBOLT_ENV",
            });
        };

        value.parse()
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
            "development" => Ok(Self::Development),
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
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn validate_runtime_config_for_mode(
    mode: RuntimeMode,
    auth_config: &AuthConfig,
    allowed_origins: &[String],
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<(), StartupError> {
    if !mode.is_production() {
        return Ok(());
    }

    for name in REQUIRED_PRODUCTION_CONFIG_VARS {
        if lookup(name).is_none_or(|value| value.trim().is_empty()) {
            return Err(StartupError::MissingProductionConfig { name });
        }
    }

    if auth_config.bootstrap_admin {
        return Err(StartupError::UnsafeProductionConfig {
            reason: "SUNBOLT_DEV_BOOTSTRAP_ADMIN must be false in production",
        });
    }

    if !auth_config.secure_cookie {
        return Err(StartupError::UnsafeProductionConfig {
            reason: "SUNBOLT_COOKIE_SECURE must be true in production",
        });
    }

    if allowed_origins.is_empty() || allowed_origins.iter().any(|origin| origin == "*") {
        return Err(StartupError::UnsafeProductionConfig {
            reason: "SUNBOLT_ALLOWED_ORIGINS must list explicit browser origins in production",
        });
    }

    Ok(())
}

fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.parse().ok()
}

fn env_duration_secs(name: &str) -> Option<Duration> {
    env_usize(name).and_then(|seconds| u64::try_from(seconds).ok().map(Duration::from_secs))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sunbolt_auth::AuthConfig;

    use super::{
        validate_runtime_config_for_mode, ProductionStateBackend, ProductionStateConfig,
        RuntimeMode,
    };

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
        assert!("".parse::<RuntimeMode>().is_err());
        assert!("preview".parse::<RuntimeMode>().is_err());
    }

    #[test]
    fn runtime_mode_requires_explicit_env_value() {
        let error = RuntimeMode::from_env_value(None).expect_err("mode should be required");

        assert!(matches!(
            error,
            crate::error::StartupError::MissingRequiredEnv {
                name: "SUNBOLT_ENV"
            }
        ));
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

    #[test]
    fn production_runtime_validation_rejects_missing_required_config() {
        let error = validate_runtime_config_for_mode(
            RuntimeMode::Production,
            &production_auth_config(),
            &["https://sunbolt.example.com".to_owned()],
            |_| None,
        )
        .expect_err("production should require explicit config");

        assert!(matches!(
            error,
            crate::error::StartupError::MissingProductionConfig {
                name: "SUNBOLT_DATABASE_URL"
            }
        ));
    }

    #[test]
    fn production_runtime_validation_rejects_bootstrap_admin() {
        let mut auth_config = production_auth_config();
        auth_config.bootstrap_admin = true;

        let error = validate_runtime_config_for_mode(
            RuntimeMode::Production,
            &auth_config,
            &["https://sunbolt.example.com".to_owned()],
            production_lookup,
        )
        .expect_err("production should reject bootstrap admin");

        assert!(matches!(
            error,
            crate::error::StartupError::UnsafeProductionConfig { .. }
        ));
        assert!(error.to_string().contains("SUNBOLT_DEV_BOOTSTRAP_ADMIN"));
    }

    #[test]
    fn production_runtime_validation_rejects_insecure_cookies() {
        let mut auth_config = production_auth_config();
        auth_config.secure_cookie = false;

        let error = validate_runtime_config_for_mode(
            RuntimeMode::Production,
            &auth_config,
            &["https://sunbolt.example.com".to_owned()],
            production_lookup,
        )
        .expect_err("production should reject insecure cookies");

        assert!(error.to_string().contains("SUNBOLT_COOKIE_SECURE"));
    }

    #[test]
    fn production_runtime_validation_rejects_permissive_origins() {
        for allowed_origins in [Vec::new(), vec!["*".to_owned()]] {
            let error = validate_runtime_config_for_mode(
                RuntimeMode::Production,
                &production_auth_config(),
                &allowed_origins,
                production_lookup,
            )
            .expect_err("production should reject permissive origins");

            assert!(error.to_string().contains("SUNBOLT_ALLOWED_ORIGINS"));
        }
    }

    #[test]
    fn production_runtime_validation_accepts_hardened_config() {
        validate_runtime_config_for_mode(
            RuntimeMode::Production,
            &production_auth_config(),
            &["https://sunbolt.example.com".to_owned()],
            production_lookup,
        )
        .expect("hardened production config should pass");
    }

    #[test]
    fn development_runtime_validation_allows_local_shortcuts() {
        let auth_config = AuthConfig {
            bootstrap_admin: true,
            secure_cookie: false,
            ..production_auth_config()
        };

        validate_runtime_config_for_mode(RuntimeMode::Development, &auth_config, &[], |_| None)
            .expect("development may use local shortcuts");
    }

    fn production_auth_config() -> AuthConfig {
        AuthConfig {
            session_ttl: Duration::from_secs(3600),
            recent_mfa_ttl: Duration::from_secs(300),
            secure_cookie: true,
            require_step_up_mfa_for_terminal: true,
            bootstrap_admin: false,
            admin_email: "admin@example.com".to_owned(),
            admin_password: "unused-development-password".to_owned(),
        }
    }

    fn production_lookup(name: &str) -> Option<String> {
        match name {
            "SUNBOLT_DATABASE_URL" => Some("postgres://sunbolt:secret@db/sunbolt".to_owned()),
            "SUNBOLT_PUBLIC_URL" => Some("https://sunbolt.example.com".to_owned()),
            _ => None,
        }
    }
}
