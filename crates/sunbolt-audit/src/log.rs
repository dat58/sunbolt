use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::chain::{compute_event_hash, verify_chain, GENESIS_HASH};
use crate::export::export_json;
use crate::redaction::redact_sensitive;
use crate::types::{AuditEvent, AuditEventInput};

/// Internal state kept under a single lock so that hash-chain writes are
/// always atomic: the previous hash, event hash, and Vec push happen together.
#[derive(Debug)]
struct AuditLogState {
    events: Vec<AuditEvent>,
    last_hash: String,
    next_id: u64,
}

impl AuditLogState {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            last_hash: GENESIS_HASH.to_owned(),
            next_id: 1,
        }
    }
}

/// Append-only in-memory audit log with SHA-256 hash chain integrity.
///
/// Each recorded event is linked to the previous one via `previous_hash` and
/// `event_hash`, forming a tamper-evident chain.  Call [`AuditLog::verify_integrity`]
/// to confirm the chain is intact.
#[derive(Debug, Clone)]
pub struct AuditLog {
    inner: Arc<Mutex<AuditLogState>>,
}

impl Default for AuditLog {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AuditLogState::new())),
        }
    }
}

impl AuditLog {
    /// Records an audit event and appends it to the hash chain.
    ///
    /// Both the chain update and the Vec push occur under the same lock,
    /// so concurrent calls always produce a consistent, linear chain.
    pub fn record(&self, input: AuditEventInput) {
        let created_at = now_unix_secs();
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let id = state.next_id;
        state.next_id += 1;

        let previous_hash = state.last_hash.clone();
        let message = redact_sensitive(&input.message).into_owned();
        let event_hash = compute_event_hash(
            id,
            input.kind.as_str(),
            input.actor_email.as_deref(),
            &message,
            created_at,
            &previous_hash,
        );
        state.last_hash.clone_from(&event_hash);
        state.events.push(AuditEvent {
            id,
            kind: input.kind,
            actor_email: input.actor_email,
            message,
            created_at_unix_secs: created_at,
            previous_hash,
            event_hash,
        });
    }

    /// Returns all audit events in append order.
    #[must_use]
    pub fn events(&self) -> Vec<AuditEvent> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .events
            .clone()
    }

    /// Returns only the subset of events relevant to the access-history view.
    #[must_use]
    pub fn access_history(&self) -> Vec<AuditEvent> {
        self.events()
            .into_iter()
            .filter(|e| e.kind.is_access_history())
            .collect()
    }

    /// Verifies the integrity of the entire hash chain.
    ///
    /// Returns `true` when every event has a valid `previous_hash` and
    /// `event_hash` linking it to its predecessor.
    #[must_use]
    pub fn verify_integrity(&self) -> bool {
        verify_chain(&self.events())
    }

    /// Exports all events as a pretty-printed JSON string.
    ///
    /// The output includes `event_hash` and `previous_hash` so external
    /// tools can re-verify the chain after export.
    #[must_use]
    pub fn export_json(&self) -> String {
        export_json(&self.events())
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
    use super::AuditLog;
    use crate::chain::GENESIS_HASH;
    use crate::types::{AuditEventInput, AuditEventKind};

    fn login_event() -> AuditEventInput {
        AuditEventInput {
            kind: AuditEventKind::UserLoginSuccess,
            actor_email: Some("admin@example.com".to_owned()),
            message: "login succeeded".to_owned(),
        }
    }

    fn terminal_event() -> AuditEventInput {
        AuditEventInput {
            kind: AuditEventKind::TerminalOpened,
            actor_email: Some("admin@example.com".to_owned()),
            message: "terminal opened".to_owned(),
        }
    }

    #[test]
    fn append_only_ids_are_sequential() {
        let log = AuditLog::default();
        log.record(login_event());
        log.record(terminal_event());

        let events = log.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, 1);
        assert_eq!(events[1].id, 2);
        assert!(events[1].created_at_unix_secs >= events[0].created_at_unix_secs);
    }

    #[test]
    fn first_event_links_to_genesis() {
        let log = AuditLog::default();
        log.record(login_event());

        let events = log.events();
        assert_eq!(events[0].previous_hash, GENESIS_HASH);
        assert_eq!(events[0].event_hash.len(), 64);
    }

    #[test]
    fn each_event_links_to_predecessor() {
        let log = AuditLog::default();
        log.record(login_event());
        log.record(terminal_event());

        let events = log.events();
        assert_eq!(events[1].previous_hash, events[0].event_hash);
    }

    #[test]
    fn verify_integrity_passes_for_fresh_log() {
        let log = AuditLog::default();
        log.record(login_event());
        log.record(terminal_event());
        assert!(log.verify_integrity());
    }

    #[test]
    fn verify_integrity_passes_for_empty_log() {
        assert!(AuditLog::default().verify_integrity());
    }

    #[test]
    fn access_history_filters_non_history_events() {
        let log = AuditLog::default();
        log.record(login_event());
        log.record(AuditEventInput {
            kind: AuditEventKind::NodeEnrolled,
            actor_email: None,
            message: "node enrolled".to_owned(),
        });
        log.record(terminal_event());

        let history = log.access_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].kind, AuditEventKind::UserLoginSuccess);
        assert_eq!(history[1].kind, AuditEventKind::TerminalOpened);
    }

    #[test]
    fn export_json_contains_all_events_and_hash_fields() {
        let log = AuditLog::default();
        log.record(login_event());

        let json = log.export_json();
        assert!(json.contains("event_hash"));
        assert!(json.contains("previous_hash"));
        assert!(json.contains("user.login.success"));
        assert!(json.contains("login succeeded"));
    }

    #[test]
    fn cloned_log_shares_state() {
        let log = AuditLog::default();
        let log2 = log.clone();
        log.record(login_event());

        assert_eq!(log2.events().len(), 1);
    }

    #[test]
    fn record_redacts_secret_material_before_storage_and_hashing() {
        let log = AuditLog::default();
        let token = "a".repeat(64);
        log.record(AuditEventInput {
            kind: AuditEventKind::NodeEnrolled,
            actor_email: Some("admin@example.com".to_owned()),
            message: format!("node enrolled with enrollment_token={token}"),
        });

        let events = log.events();
        assert_eq!(
            events[0].message,
            "node enrolled with enrollment_token=[REDACTED]"
        );
        assert!(!events[0].message.contains(&token));
        assert!(log.verify_integrity());
    }
}
