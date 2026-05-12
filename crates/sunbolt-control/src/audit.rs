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

pub(crate) fn record_route_selected(
    audit: &AuditLog,
    actor_email: &str,
    node_id: &str,
    session_id: &str,
    route_id: &str,
) {
    record_event(
        audit,
        AuditEventKind::RouteSelected,
        Some(actor_email.to_owned()),
        format!("route {route_id} selected for terminal session {session_id} on node {node_id}"),
    );
}

pub(crate) fn record_route_failed(
    audit: &AuditLog,
    actor_email: &str,
    node_id: &str,
    session_id: &str,
    route_id: Option<&str>,
    reason: &str,
) {
    let route_id = route_id.unwrap_or("unavailable");
    record_event(
        audit,
        AuditEventKind::RouteFailed,
        Some(actor_email.to_owned()),
        format!(
            "route {route_id} failed for terminal session {session_id} on node {node_id}: {reason}"
        ),
    );
}

#[cfg(test)]
mod tests {
    use sunbolt_audit::{AuditEventKind, AuditLog};

    use super::{record_route_failed, record_route_selected};

    #[test]
    fn route_selection_and_failure_events_use_stable_taxonomy() {
        let audit = AuditLog::default();

        record_route_selected(
            &audit,
            "admin@example.com",
            "node-1",
            "session-1",
            "direct-agent:node-1",
        );
        record_route_failed(
            &audit,
            "admin@example.com",
            "node-1",
            "session-1",
            Some("direct-agent:node-1"),
            "agent connection dropped",
        );

        let events = audit.events();
        assert_eq!(events[0].kind, AuditEventKind::RouteSelected);
        assert!(events[0].message.contains("direct-agent:node-1"));
        assert_eq!(events[1].kind, AuditEventKind::RouteFailed);
        assert!(events[1].message.contains("agent connection dropped"));
    }
}
