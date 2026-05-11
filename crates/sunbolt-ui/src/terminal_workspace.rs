use sunbolt_protocol::TerminalSize;

/// DOM id used by the browser terminal bridge.
pub const TERMINAL_MOUNT_ID: &str = "sunbolt-terminal";
pub const TERMINAL_NODE_INPUT_ID: &str = "sunbolt-terminal-node";

pub const TERMINAL_STATUS_ID: &str = "sunbolt-terminal-status";
pub const TERMINAL_ERROR_ID: &str = "sunbolt-terminal-error";
pub const TERMINAL_AUTH_PANEL_ID: &str = "sunbolt-terminal-auth";
pub const TERMINAL_TABS_ID: &str = "sunbolt-terminal-tabs";
pub const TERMINAL_DETACHED_SESSIONS_ID: &str = "sunbolt-terminal-detached-sessions";

pub const DEFAULT_TERMINAL_SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalWorkspacePanel {
    Terminal,
    DetachedSessions,
    SessionActions,
    NodeSelector,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalWorkspaceState {
    pub active_panel: TerminalWorkspacePanel,
    pub active_session_count: usize,
    pub detached_session_count: usize,
}

impl Default for TerminalWorkspaceState {
    fn default() -> Self {
        Self {
            active_panel: TerminalWorkspacePanel::Terminal,
            active_session_count: 0,
            detached_session_count: 0,
        }
    }
}
