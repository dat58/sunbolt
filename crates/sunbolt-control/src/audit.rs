use sunbolt_audit::{AuditEventInput, AuditEventKind, AuditLog};

pub(crate) fn record_event(
    audit: &AuditLog,
    kind: AuditEventKind,
    actor_email: Option<String>,
    message: impl Into<String>,
) {
    audit.record(AuditEventInput {
        kind,
        actor_email,
        message: message.into(),
    });
}

pub(crate) fn record_terminal_closed(
    audit: &AuditLog,
    actor_email: String,
    message: impl Into<String>,
) {
    record_event(
        audit,
        AuditEventKind::TerminalClosed,
        Some(actor_email),
        message,
    );
}
