use sha2::{Digest, Sha256};

use crate::types::AuditEvent;

/// Hash used as the `previous_hash` of the very first event in the chain.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Computes the SHA-256 hash that covers both the event content and the
/// previous event's hash, binding this event into the chain.
///
/// The input is canonicalised as:
/// `"{previous_hash}:{id}:{kind}:{actor_email}:{message}:{created_at_unix_secs}"`
pub fn compute_event_hash(
    id: u64,
    kind_str: &str,
    actor_email: Option<&str>,
    message: &str,
    created_at_unix_secs: u64,
    previous_hash: &str,
) -> String {
    let canonical = format!(
        "{previous_hash}:{id}:{kind_str}:{}:{message}:{created_at_unix_secs}",
        actor_email.unwrap_or(""),
    );
    hex_sha256(canonical.as_bytes())
}

/// Verifies that every event in `events` has the correct `previous_hash` and
/// `event_hash`, walking the chain from the genesis hash.
///
/// Returns `true` if the chain is intact, `false` if any event has been
/// added out of order, removed, or had its content modified.
#[must_use]
pub fn verify_chain(events: &[AuditEvent]) -> bool {
    let mut expected_previous = GENESIS_HASH.to_owned();
    for event in events {
        if event.previous_hash != expected_previous {
            return false;
        }
        let expected_hash = compute_event_hash(
            event.id,
            event.kind.as_str(),
            event.actor_email.as_deref(),
            &event.message,
            event.created_at_unix_secs,
            &event.previous_hash,
        );
        if event.event_hash != expected_hash {
            return false;
        }
        expected_previous.clone_from(&event.event_hash);
    }
    true
}

fn hex_sha256(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(64);
    for byte in &digest {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{compute_event_hash, verify_chain, GENESIS_HASH};
    use crate::types::{AuditEvent, AuditEventKind};

    fn make_event(
        id: u64,
        previous_hash: String,
        message: &str,
        created_at_unix_secs: u64,
    ) -> AuditEvent {
        let event_hash = compute_event_hash(
            id,
            AuditEventKind::UserLoginSuccess.as_str(),
            Some("test@example.com"),
            message,
            created_at_unix_secs,
            &previous_hash,
        );
        AuditEvent {
            id,
            kind: AuditEventKind::UserLoginSuccess,
            actor_email: Some("test@example.com".to_owned()),
            message: message.to_owned(),
            created_at_unix_secs,
            previous_hash,
            event_hash,
        }
    }

    #[test]
    fn compute_event_hash_is_deterministic() {
        let h1 = compute_event_hash(1, "user.login.success", None, "msg", 0, GENESIS_HASH);
        let h2 = compute_event_hash(1, "user.login.success", None, "msg", 0, GENESIS_HASH);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn compute_event_hash_differs_for_different_inputs() {
        let base = compute_event_hash(1, "user.login.success", None, "msg", 0, GENESIS_HASH);
        let diff_id = compute_event_hash(2, "user.login.success", None, "msg", 0, GENESIS_HASH);
        let diff_msg = compute_event_hash(1, "user.login.success", None, "other", 0, GENESIS_HASH);
        let diff_prev = compute_event_hash(1, "user.login.success", None, "msg", 0, "deadbeef");
        assert_ne!(base, diff_id);
        assert_ne!(base, diff_msg);
        assert_ne!(base, diff_prev);
    }

    #[test]
    fn verify_chain_passes_for_empty_log() {
        assert!(verify_chain(&[]));
    }

    #[test]
    fn verify_chain_passes_for_valid_chain() {
        let e1 = make_event(1, GENESIS_HASH.to_owned(), "first", 1000);
        let e2 = make_event(2, e1.event_hash.clone(), "second", 1001);
        let e3 = make_event(3, e2.event_hash.clone(), "third", 1002);
        assert!(verify_chain(&[e1, e2, e3]));
    }

    #[test]
    fn verify_chain_fails_for_wrong_previous_hash() {
        let mut e1 = make_event(1, GENESIS_HASH.to_owned(), "first", 1000);
        let e2 = make_event(2, e1.event_hash.clone(), "second", 1001);
        e1.previous_hash = "tampered".to_owned();
        assert!(!verify_chain(&[e1, e2]));
    }

    #[test]
    fn verify_chain_fails_for_tampered_event_content() {
        let e1 = make_event(1, GENESIS_HASH.to_owned(), "first", 1000);
        let mut e2 = make_event(2, e1.event_hash.clone(), "second", 1001);
        e2.message = "tampered message".to_owned();
        assert!(!verify_chain(&[e1, e2]));
    }

    #[test]
    fn verify_chain_fails_for_tampered_event_hash() {
        let e1 = make_event(1, GENESIS_HASH.to_owned(), "first", 1000);
        let mut e2 = make_event(2, e1.event_hash.clone(), "second", 1001);
        e2.event_hash = "0".repeat(64);
        assert!(!verify_chain(&[e1, e2]));
    }
}
