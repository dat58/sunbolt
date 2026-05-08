use std::{env, time::Duration};

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
