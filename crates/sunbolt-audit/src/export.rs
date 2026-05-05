use crate::types::AuditEvent;

/// Serialises the audit event slice as a pretty-printed JSON array.
///
/// The output includes `event_hash` and `previous_hash` fields so that
/// external tools can independently verify chain integrity after export.
///
/// # Panics
///
/// Panics if `AuditEvent` fails to serialise, which cannot happen in practice
/// because all field types implement `Serialize` without fallible paths.
#[must_use]
pub fn export_json(events: &[AuditEvent]) -> String {
    serde_json::to_string_pretty(events).expect("audit events always serialize to JSON")
}

#[cfg(test)]
mod tests {
    use super::export_json;
    use crate::types::{AuditEvent, AuditEventKind};

    fn sample_event() -> AuditEvent {
        AuditEvent {
            id: 1,
            kind: AuditEventKind::UserLoginSuccess,
            actor_email: Some("user@example.com".to_owned()),
            message: "login succeeded".to_owned(),
            created_at_unix_secs: 1_000_000,
            previous_hash: "0".repeat(64),
            event_hash: "a".repeat(64),
        }
    }

    #[test]
    fn export_json_produces_valid_json_array() {
        let json = export_json(&[sample_event()]);
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("export should produce valid JSON");
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 1);
    }

    #[test]
    fn export_json_includes_hash_fields() {
        let json = export_json(&[sample_event()]);
        assert!(json.contains("event_hash"));
        assert!(json.contains("previous_hash"));
    }

    #[test]
    fn export_json_of_empty_slice_is_empty_array() {
        let json = export_json(&[]);
        assert_eq!(json.trim(), "[]");
    }
}
