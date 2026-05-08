use crate::repositories::{auth::UserId, DurableRepository, DurableStateKind, RepositoryFuture};

pub type AuditEventId = i64;

/// Optional durable audit target.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AuditEventTarget {
    pub target_type: String,
    pub target_id: String,
}

/// Durable audit event record.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AuditEventRecord {
    pub id: AuditEventId,
    pub user_id: Option<UserId>,
    pub event_type: String,
    pub target: Option<AuditEventTarget>,
    pub metadata_json: Option<String>,
    pub ip_address: Option<String>,
    pub created_at_unix_secs: i64,
}

/// Input for append-only audit event writes.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AuditEventInput {
    pub user_id: Option<UserId>,
    pub event_type: String,
    pub target: Option<AuditEventTarget>,
    pub metadata_json: Option<String>,
    pub ip_address: Option<String>,
}

/// Repository boundary for append-only audit event storage.
pub trait AuditEventRepository: DurableRepository {
    /// Appends an audit event record.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot persist the event.
    fn append_event(&self, input: AuditEventInput) -> RepositoryFuture<'_, AuditEventRecord>;

    /// Lists recent audit events for audit and access-history views.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot list events.
    fn list_recent_events(&self, limit: u32) -> RepositoryFuture<'_, Vec<AuditEventRecord>>;
}

/// Marker repository for durable audit events.
pub struct AuditEventRepositoryBoundary;

impl DurableRepository for AuditEventRepositoryBoundary {
    const STATE_KIND: DurableStateKind = DurableStateKind::AuditEvents;
}

#[cfg(test)]
mod tests {
    use super::AuditEventRepositoryBoundary;
    use crate::repositories::{DurableRepository, DurableStateKind};

    #[test]
    fn audit_repository_marker_maps_to_durable_state() {
        assert_eq!(
            AuditEventRepositoryBoundary.state_kind(),
            DurableStateKind::AuditEvents
        );
    }
}
