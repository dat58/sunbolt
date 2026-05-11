/// WebSocket endpoint used by the terminal UI.
pub const TERMINAL_WS_ENDPOINT: &str = "/terminal/ws";
pub const TERMINAL_ACTIVE_SESSIONS_ENDPOINT: &str = "/terminal/sessions/active";
pub const TERMINAL_DETACHED_SESSIONS_ENDPOINT: &str = "/terminal/sessions/detached";
pub const TERMINAL_SESSION_TERMINATE_PREFIX: &str = "/terminal/sessions";
pub const AUTH_LOGIN_ENDPOINT: &str = "/auth/login";
pub const AUTH_ME_ENDPOINT: &str = "/auth/me";
pub const AUTH_TERMINAL_ACCESS_ENDPOINT: &str = "/auth/terminal-access";
pub const STEP_UP_MFA_ENDPOINT: &str = "/auth/mfa/step-up";
pub const CONTROL_PLANE_URL_CONFIG_GLOBAL: &str = "SUNBOLT_CONTROL_PLANE_URL";
pub const TERMINAL_WS_CONFIG_GLOBAL: &str = "SUNBOLT_TERMINAL_WS_URL";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiEndpoint {
    AuthLogin,
    AuthMe,
    TerminalAccess,
    StepUpMfa,
    TerminalActiveSessions,
    TerminalDetachedSessions,
    TerminalWebSocket,
}

impl ApiEndpoint {
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::AuthLogin => AUTH_LOGIN_ENDPOINT,
            Self::AuthMe => AUTH_ME_ENDPOINT,
            Self::TerminalAccess => AUTH_TERMINAL_ACCESS_ENDPOINT,
            Self::StepUpMfa => STEP_UP_MFA_ENDPOINT,
            Self::TerminalActiveSessions => TERMINAL_ACTIVE_SESSIONS_ENDPOINT,
            Self::TerminalDetachedSessions => TERMINAL_DETACHED_SESSIONS_ENDPOINT,
            Self::TerminalWebSocket => TERMINAL_WS_ENDPOINT,
        }
    }
}

#[must_use]
pub(crate) fn control_plane_config_script() -> Option<String> {
    browser_config_script(option_env!("SUNBOLT_CONTROL_PLANE_URL"))
}

#[must_use]
pub(crate) fn browser_config_script(control_plane_url: Option<&str>) -> Option<String> {
    let control_plane_url = control_plane_url?.trim();
    if control_plane_url.is_empty() {
        return None;
    }

    Some(format!(
        r#"window.{CONTROL_PLANE_URL_CONFIG_GLOBAL} = window.{CONTROL_PLANE_URL_CONFIG_GLOBAL} || "{control_plane_url}";"#
    ))
}
