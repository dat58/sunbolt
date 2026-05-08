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
    use std::future;

    use super::{
        AuditEventInput, AuditEventRecord, AuditEventRepository, AuditEventRepositoryBoundary,
        AuditEventTarget,
    };
    use crate::repositories::{DurableRepository, DurableStateKind, RepositoryFuture};

    struct MockAuditEventRepository;

    impl DurableRepository for MockAuditEventRepository {
        const STATE_KIND: DurableStateKind = DurableStateKind::AuditEvents;
    }

    impl AuditEventRepository for MockAuditEventRepository {
        fn append_event(&self, input: AuditEventInput) -> RepositoryFuture<'_, AuditEventRecord> {
            Box::pin(future::ready(Ok(AuditEventRecord {
                id: 1,
                user_id: input.user_id,
                event_type: input.event_type,
                target: input.target,
                metadata_json: input.metadata_json,
                ip_address: input.ip_address,
                created_at_unix_secs: 1,
            })))
        }

        fn list_recent_events(&self, limit: u32) -> RepositoryFuture<'_, Vec<AuditEventRecord>> {
            Box::pin(future::ready(Ok((0..limit)
                .map(|index| AuditEventRecord {
                    id: i64::from(index) + 1,
                    user_id: Some(7),
                    event_type: "terminal.opened".to_owned(),
                    target: Some(AuditEventTarget {
                        target_type: "terminal_session".to_owned(),
                        target_id: format!("session-{index}"),
                    }),
                    metadata_json: None,
                    ip_address: None,
                    created_at_unix_secs: i64::from(index) + 1,
                })
                .collect())))
        }
    }

    #[test]
    fn audit_repository_marker_maps_to_durable_state() {
        assert_eq!(
            AuditEventRepositoryBoundary.state_kind(),
            DurableStateKind::AuditEvents
        );
    }

    #[tokio::test]
    async fn audit_event_repository_boundary_can_be_mocked() {
        let repo = MockAuditEventRepository;
        let event = repo
            .append_event(AuditEventInput {
                user_id: Some(7),
                event_type: "terminal.opened".to_owned(),
                target: Some(AuditEventTarget {
                    target_type: "terminal_session".to_owned(),
                    target_id: "session-1".to_owned(),
                }),
                metadata_json: None,
                ip_address: Some("127.0.0.1".to_owned()),
            })
            .await
            .expect("mock audit append succeeds");

        assert_eq!(event.event_type, "terminal.opened");
        assert_eq!(repo.state_kind(), DurableStateKind::AuditEvents);
    }
}
