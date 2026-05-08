use sunbolt_audit::AuditLog;
use sunbolt_auth::AuthService;

use crate::{
    agent::AgentConnectionRegistry,
    config::{
        allowed_origins_from_env, TerminalSessionConfig, DEFAULT_LOGIN_RATE_MAX,
        DEFAULT_LOGIN_RATE_WINDOW, DEFAULT_TERMINAL_RATE_MAX, DEFAULT_TERMINAL_RATE_WINDOW,
    },
    node::NodeEnrollmentRegistry,
    rate_limit::SlidingWindowRateLimiter,
    routing::InMemoryNodeRouter,
    terminal::TerminalSessionRegistry,
};

#[derive(Clone)]
pub(crate) struct AppState {
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

impl AppState {
    pub(crate) fn from_env() -> Self {
        Self {
            sessions: TerminalSessionRegistry::default(),
            terminal_config: TerminalSessionConfig::from_env(),
            auth: AuthService::from_env(),
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
}
