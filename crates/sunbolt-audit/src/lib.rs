use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

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
    TerminalClosed,
    TerminalFailed,
    NodeEnrolled,
    NodeRevoked,
}

impl AuditEventKind {
    /// Returns the stable event name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserLoginSuccess => "user.login.success",
            Self::UserLoginFailed => "user.login.failed",
            Self::UserLogout => "user.logout",
            Self::UserMfaChallenge => "user.mfa.challenge",
            Self::UserMfaSuccess => "user.mfa.success",
            Self::TerminalOpened => "terminal.opened",
            Self::TerminalClosed => "terminal.closed",
            Self::TerminalFailed => "terminal.failed",
            Self::NodeEnrolled => "node.enrolled",
            Self::NodeRevoked => "node.revoked",
        }
    }

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
                | Self::TerminalClosed
                | Self::TerminalFailed
        )
    }
}

/// Audit event record.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: u64,
    pub kind: AuditEventKind,
    pub actor_email: Option<String>,
    pub message: String,
    pub created_at_unix_secs: u64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AuditEventInput {
    pub kind: AuditEventKind,
    pub actor_email: Option<String>,
    pub message: String,
}

/// Append-only in-memory audit writer for MVP flows.
#[derive(Debug, Clone)]
pub struct AuditLog {
    events: Arc<Mutex<Vec<AuditEvent>>>,
    next_id: Arc<AtomicU64>,
}

impl Default for AuditLog {
    fn default() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl AuditLog {
    /// Records an audit event.
    pub fn record(&self, input: AuditEventInput) {
        let mut events = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        events.push(AuditEvent {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            kind: input.kind,
            actor_email: input.actor_email,
            message: input.message,
            created_at_unix_secs: now_unix_secs(),
        });
    }

    /// Returns all audit events in append order.
    #[must_use]
    pub fn events(&self) -> Vec<AuditEvent> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Returns only access-history events in append order.
    #[must_use]
    pub fn access_history(&self) -> Vec<AuditEvent> {
        self.events()
            .into_iter()
            .filter(|event| event.kind.is_access_history())
            .collect()
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::{AuditEventInput, AuditEventKind, AuditLog};

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
        assert_eq!(AuditEventKind::TerminalClosed.as_str(), "terminal.closed");
        assert_eq!(AuditEventKind::TerminalFailed.as_str(), "terminal.failed");
        assert_eq!(AuditEventKind::NodeEnrolled.as_str(), "node.enrolled");
        assert_eq!(AuditEventKind::NodeRevoked.as_str(), "node.revoked");
    }

    #[test]
    fn audit_log_is_append_only() {
        let log = AuditLog::default();
        log.record(AuditEventInput {
            kind: AuditEventKind::UserLoginSuccess,
            actor_email: Some("admin@example.com".to_owned()),
            message: "login succeeded".to_owned(),
        });
        log.record(AuditEventInput {
            kind: AuditEventKind::TerminalOpened,
            actor_email: Some("admin@example.com".to_owned()),
            message: "terminal opened".to_owned(),
        });

        let events = log.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, 1);
        assert_eq!(events[1].id, 2);
        assert!(events[1].created_at_unix_secs >= events[0].created_at_unix_secs);
    }

    #[test]
    fn access_history_filters_events() {
        let log = AuditLog::default();
        log.record(AuditEventInput {
            kind: AuditEventKind::UserLoginSuccess,
            actor_email: Some("admin@example.com".to_owned()),
            message: "login succeeded".to_owned(),
        });
        log.record(AuditEventInput {
            kind: AuditEventKind::TerminalClosed,
            actor_email: Some("admin@example.com".to_owned()),
            message: "terminal closed".to_owned(),
        });

        let history = log.access_history();
        assert_eq!(history.len(), 2);
    }
}
