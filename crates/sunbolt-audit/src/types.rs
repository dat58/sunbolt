use serde::{Deserialize, Serialize};

/// Stable audit event names reserved by the audit boundary.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuditEventKind {
    UserLoginSuccess,
    UserLoginFailed,
    UserLogout,
    UserMfaChallenge,
    UserMfaSuccess,
    TerminalOpened,
    TerminalDetached,
    TerminalReattached,
    TerminalTerminated,
    TerminalClosed,
    TerminalFailed,
    TransportNegotiated,
    AgentConnected,
    AgentDisconnected,
    AgentAuthenticationFailed,
    NodeEnrolled,
    NodeRevoked,
}

impl AuditEventKind {
    /// Returns the stable event name used in serialized output and chain hashing.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserLoginSuccess => "user.login.success",
            Self::UserLoginFailed => "user.login.failed",
            Self::UserLogout => "user.logout",
            Self::UserMfaChallenge => "user.mfa.challenge",
            Self::UserMfaSuccess => "user.mfa.success",
            Self::TerminalOpened => "terminal.opened",
            Self::TerminalDetached => "terminal.detached",
            Self::TerminalReattached => "terminal.reattached",
            Self::TerminalTerminated => "terminal.terminated",
            Self::TerminalClosed => "terminal.closed",
            Self::TerminalFailed => "terminal.failed",
            Self::TransportNegotiated => "transport.negotiated",
            Self::AgentConnected => "agent.connected",
            Self::AgentDisconnected => "agent.disconnected",
            Self::AgentAuthenticationFailed => "agent.authentication.failed",
            Self::NodeEnrolled => "node.enrolled",
            Self::NodeRevoked => "node.revoked",
        }
    }

    /// Returns true for event kinds shown in the access-history view.
    #[must_use]
    pub const fn is_access_history(self) -> bool {
        matches!(
            self,
            Self::UserLoginSuccess
                | Self::UserLoginFailed
                | Self::UserLogout
                | Self::UserMfaChallenge
                | Self::UserMfaSuccess
                | Self::TerminalOpened
                | Self::TerminalDetached
                | Self::TerminalReattached
                | Self::TerminalTerminated
                | Self::TerminalClosed
                | Self::TerminalFailed
        )
    }
}

/// Immutable audit event record stored in the chain.
///
/// `previous_hash` links this event to its predecessor and `event_hash`
/// covers the full content of this event, enabling chain verification.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: u64,
    pub kind: AuditEventKind,
    pub actor_email: Option<String>,
    pub message: String,
    pub created_at_unix_secs: u64,
    pub previous_hash: String,
    pub event_hash: String,
}

/// Input for recording a new audit event.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AuditEventInput {
    pub kind: AuditEventKind,
    pub actor_email: Option<String>,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::AuditEventKind;

    #[test]
    fn audit_event_kind_names_are_stable() {
        assert_eq!(
            AuditEventKind::UserLoginSuccess.as_str(),
            "user.login.success"
        );
        assert_eq!(
            AuditEventKind::UserLoginFailed.as_str(),
            "user.login.failed"
        );
        assert_eq!(AuditEventKind::UserLogout.as_str(), "user.logout");
        assert_eq!(
            AuditEventKind::UserMfaChallenge.as_str(),
            "user.mfa.challenge"
        );
        assert_eq!(AuditEventKind::UserMfaSuccess.as_str(), "user.mfa.success");
        assert_eq!(AuditEventKind::TerminalOpened.as_str(), "terminal.opened");
        assert_eq!(
            AuditEventKind::TerminalDetached.as_str(),
            "terminal.detached"
        );
        assert_eq!(
            AuditEventKind::TerminalReattached.as_str(),
            "terminal.reattached"
        );
        assert_eq!(
            AuditEventKind::TerminalTerminated.as_str(),
            "terminal.terminated"
        );
        assert_eq!(AuditEventKind::TerminalClosed.as_str(), "terminal.closed");
        assert_eq!(AuditEventKind::TerminalFailed.as_str(), "terminal.failed");
        assert_eq!(
            AuditEventKind::TransportNegotiated.as_str(),
            "transport.negotiated"
        );
        assert_eq!(AuditEventKind::AgentConnected.as_str(), "agent.connected");
        assert_eq!(
            AuditEventKind::AgentDisconnected.as_str(),
            "agent.disconnected"
        );
        assert_eq!(
            AuditEventKind::AgentAuthenticationFailed.as_str(),
            "agent.authentication.failed"
        );
        assert_eq!(AuditEventKind::NodeEnrolled.as_str(), "node.enrolled");
        assert_eq!(AuditEventKind::NodeRevoked.as_str(), "node.revoked");
    }

    #[test]
    fn access_history_kinds_are_user_and_terminal_events() {
        assert!(AuditEventKind::UserLoginSuccess.is_access_history());
        assert!(AuditEventKind::TerminalOpened.is_access_history());
        assert!(AuditEventKind::TerminalDetached.is_access_history());
        assert!(AuditEventKind::TerminalReattached.is_access_history());
        assert!(AuditEventKind::TerminalTerminated.is_access_history());
        assert!(AuditEventKind::TerminalClosed.is_access_history());
        assert!(!AuditEventKind::NodeEnrolled.is_access_history());
        assert!(!AuditEventKind::NodeRevoked.is_access_history());
    }
}
