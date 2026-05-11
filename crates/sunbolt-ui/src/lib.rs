pub mod api_client;
pub mod browser_bridge;
pub mod components;
mod pages;
pub mod shell;
pub mod terminal_workspace;

pub use api_client::{
    AUTH_LOGIN_ENDPOINT, AUTH_ME_ENDPOINT, AUTH_TERMINAL_ACCESS_ENDPOINT,
    CONTROL_PLANE_URL_CONFIG_GLOBAL, STEP_UP_MFA_ENDPOINT, TERMINAL_ACTIVE_SESSIONS_ENDPOINT,
    TERMINAL_DETACHED_SESSIONS_ENDPOINT, TERMINAL_SESSION_TERMINATE_PREFIX,
    TERMINAL_WS_CONFIG_GLOBAL, TERMINAL_WS_ENDPOINT,
};
pub use browser_bridge::{terminal_bridge_script, XTERM_SCRIPT_URL, XTERM_STYLESHEET_URL};
pub use shell::{app_title, App};
pub use terminal_workspace::{TERMINAL_MOUNT_ID, TERMINAL_NODE_INPUT_ID};

#[cfg(test)]
mod tests {
    use crate::api_client::{
        browser_config_script, ApiEndpoint, AUTH_LOGIN_ENDPOINT, AUTH_ME_ENDPOINT,
        AUTH_TERMINAL_ACCESS_ENDPOINT, CONTROL_PLANE_URL_CONFIG_GLOBAL, STEP_UP_MFA_ENDPOINT,
        TERMINAL_ACTIVE_SESSIONS_ENDPOINT, TERMINAL_DETACHED_SESSIONS_ENDPOINT,
        TERMINAL_SESSION_TERMINATE_PREFIX, TERMINAL_WS_CONFIG_GLOBAL, TERMINAL_WS_ENDPOINT,
    };
    use crate::browser_bridge::{terminal_bridge_script, XTERM_SCRIPT_URL, XTERM_STYLESHEET_URL};
    use crate::components::{
        bottom_sheet, button_class, dialog, form, layout, status_badge_class, table_list,
        ButtonVariant, StatusTone,
    };
    use crate::shell::app_title;
    use crate::terminal_workspace::{
        TerminalWorkspacePanel, TerminalWorkspaceState, TERMINAL_MOUNT_ID, TERMINAL_NODE_INPUT_ID,
    };

    #[test]
    fn app_title_uses_product_name() {
        assert_eq!(app_title(), "Sunbolt");
    }

    #[test]
    fn browser_config_script_sets_control_plane_global() {
        let script = browser_config_script(Some("http://127.0.0.1:3000"))
            .expect("script should be generated");

        assert!(script.contains(CONTROL_PLANE_URL_CONFIG_GLOBAL));
        assert!(script.contains("http://127.0.0.1:3000"));
    }

    #[test]
    fn browser_config_script_ignores_empty_urls() {
        assert!(browser_config_script(Some("   ")).is_none());
        assert!(browser_config_script(None).is_none());
    }

    #[test]
    fn api_endpoint_paths_are_centralized() {
        assert_eq!(ApiEndpoint::AuthLogin.path(), AUTH_LOGIN_ENDPOINT);
        assert_eq!(ApiEndpoint::AuthMe.path(), AUTH_ME_ENDPOINT);
        assert_eq!(
            ApiEndpoint::TerminalAccess.path(),
            AUTH_TERMINAL_ACCESS_ENDPOINT
        );
        assert_eq!(ApiEndpoint::StepUpMfa.path(), STEP_UP_MFA_ENDPOINT);
        assert_eq!(
            ApiEndpoint::TerminalActiveSessions.path(),
            TERMINAL_ACTIVE_SESSIONS_ENDPOINT
        );
        assert_eq!(
            ApiEndpoint::TerminalDetachedSessions.path(),
            TERMINAL_DETACHED_SESSIONS_ENDPOINT
        );
        assert_eq!(ApiEndpoint::TerminalWebSocket.path(), TERMINAL_WS_ENDPOINT);
    }

    #[test]
    fn reusable_component_classes_are_stable() {
        assert_eq!(
            button_class(ButtonVariant::Primary),
            "sunbolt-button sunbolt-button-primary"
        );
        assert_eq!(
            button_class(ButtonVariant::Navigation { active: true }),
            "sunbolt-nav-button sunbolt-nav-button-active"
        );
        assert!(status_badge_class(StatusTone::Connected).contains("lightning-cyan"));
        assert_eq!(table_list::TABLE, "sunbolt-table");
        assert_eq!(table_list::TOOLBAR, "sunbolt-table-toolbar");
        assert_eq!(table_list::PAGINATION, "sunbolt-pagination");
        assert_eq!(table_list::DENSE_LIST, "sunbolt-dense-list");
        assert_eq!(form::TEXT_INPUT, "sunbolt-input");
        assert_eq!(dialog::MODAL, "sunbolt-modal");
        assert_eq!(bottom_sheet::SHEET, "sunbolt-bottom-sheet");
        assert_eq!(layout::DASHBOARD_GRID, "sunbolt-dashboard-grid");
    }

    #[test]
    fn terminal_workspace_state_defaults_to_terminal_panel() {
        let state = TerminalWorkspaceState::default();

        assert_eq!(state.active_panel, TerminalWorkspacePanel::Terminal);
        assert_eq!(state.active_session_count, 0);
        assert_eq!(state.detached_session_count, 0);
    }

    #[test]
    fn terminal_bridge_uses_terminal_websocket_endpoint() {
        let script = terminal_bridge_script();

        assert!(script.contains(TERMINAL_WS_ENDPOINT));
        assert!(script.contains(AUTH_LOGIN_ENDPOINT));
        assert!(script.contains(AUTH_ME_ENDPOINT));
        assert!(script.contains(AUTH_TERMINAL_ACCESS_ENDPOINT));
        assert!(script.contains(TERMINAL_ACTIVE_SESSIONS_ENDPOINT));
        assert!(script.contains(TERMINAL_DETACHED_SESSIONS_ENDPOINT));
        assert!(script.contains(TERMINAL_SESSION_TERMINATE_PREFIX));
        assert!(script.contains(STEP_UP_MFA_ENDPOINT));
        assert!(script.contains(TERMINAL_MOUNT_ID));
        assert!(script.contains(TERMINAL_NODE_INPUT_ID));
        assert!(script.contains(r#"type: "start""#));
        assert!(script.contains(r#"type: "input""#));
        assert!(script.contains(r#"type: "resize""#));
        assert!(script.contains(r#"type: "terminate""#));
        assert!(script.contains(r#"type: "detach""#));
        assert!(script.contains("sunbolt-terminal-close"));
        assert!(script.contains("sunbolt-terminal-close-tab"));
        assert!(script.contains("sunbolt-terminal-tabs"));
        assert!(script.contains("sunbolt-terminal-mfa"));
        assert!(script.contains("sunbolt-terminal-reconnect"));
        assert!(script.contains("border-lightning-cyan"));
    }

    #[test]
    fn terminal_bridge_uses_xterm_renderer() {
        let script = terminal_bridge_script();

        assert!(XTERM_SCRIPT_URL.contains("@xterm/xterm@5.5.0"));
        assert!(XTERM_STYLESHEET_URL.contains("@xterm/xterm@5.5.0"));
        assert!(script.contains("window.Terminal"));
        assert!(script.contains("loadXterm"));
        assert!(script.contains(XTERM_SCRIPT_URL));
        assert!(script.contains("terminal.open(mount)"));
        assert!(script.contains("terminal.write(safeData)"));
        assert!(script.contains("terminal.focus()"));
        assert!(script.contains("sunboltTerminalRenderer"));
    }

    #[test]
    fn terminal_bridge_exposes_websocket_client_states() {
        let script = terminal_bridge_script();

        assert!(script.contains(CONTROL_PLANE_URL_CONFIG_GLOBAL));
        assert!(script.contains(TERMINAL_WS_CONFIG_GLOBAL));
        assert!(script.contains("controlPlaneBaseUrl"));
        assert!(script.contains(r#"window.location.port !== "3000""#));
        assert!(script.contains("httpEndpointUrl"));
        assert!(script.contains("ensureTerminalAccess"));
        assert!(script.contains("terminalWebSocketUrl"));
        assert!(script.contains("new WebSocket(url)"));
        assert!(script.contains(r#"setStatus("Idle", "idle")"#));
        assert!(script.contains(r#"setStatus("Checking Session", "connecting")"#));
        assert!(script.contains(r#"setStatus("Login Required", "idle")"#));
        assert!(script.contains(r#"setStatus("MFA Required", "error")"#));
        assert!(script.contains(r#"setStatus("Connecting", "connecting")"#));
        assert!(script.contains(r#"setStatus("Active", "connected")"#));
        assert!(script.contains(r#"setStatus(label, "degraded")"#));
        assert!(script.contains("Degraded: Long Poll"));
        assert!(script.contains(r#"setStatus("Disconnected", "disconnected")"#));
        assert!(script.contains(r#"setStatus("Error", "error")"#));
        assert!(script.contains("sunbolt-terminal-error"));
        assert!(script.contains("sunbolt-terminal-retry"));
    }

    #[test]
    fn terminal_bridge_requires_auth_before_opening_websocket() {
        let script = terminal_bridge_script();

        assert!(script.contains("ensureAuthenticatedSession"));
        assert!(script.contains(r#"fetch(httpEndpointUrl("/auth/me")"#));
        assert!(script.contains(r#"fetch(httpEndpointUrl("/auth/login")"#));
        assert!(script.contains("credentials: \"include\""));
        assert!(script.contains("Sign in before opening a terminal."));
    }

    #[test]
    fn terminal_bridge_sanitizes_terminal_data_without_blocking_control_input() {
        let script = terminal_bridge_script();

        assert!(script.contains("terminalData"));
        assert!(script.contains("stripTerminalControls"));
        assert!(script.contains(r#"replace(/\u0000/g, "")"#));
        assert!(script.contains("terminal.onData(sendInput)"));
        assert!(script.contains(r#"type: "input""#));
        assert!(script.contains("writeOutput(message.data)"));
    }

    #[test]
    fn terminal_bridge_fallback_keeps_command_entry_and_keyboard_controls() {
        let script = terminal_bridge_script();

        assert!(script.contains("Type a command and press Enter"));
        assert!(script.contains("keydown"));
        assert!(script.contains("ArrowUp"));
        assert!(script.contains("ArrowDown"));
        assert!(script.contains(r#"event.key === "Tab""#));
        assert!(script.contains(r#"event.key === "Enter""#));
        assert!(script.contains("fallbackInput.value = \"\""));
    }

    #[test]
    fn terminal_bridge_handles_resize_and_cleanup() {
        let script = terminal_bridge_script();

        assert!(script.contains("new ResizeObserver(resize)"));
        assert!(script.contains(r#"type: "resize""#));
        assert!(script.contains("new MutationObserver"));
        assert!(script.contains("cleanupTerminal"));
        assert!(script.contains(r#"type: "detach""#));
        assert!(script.contains(r#"type: "terminate""#));
        assert!(script.contains("pagehide"));
        assert!(script.contains("beforeunload"));
    }
}
