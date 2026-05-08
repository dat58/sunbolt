use crate::repositories::{auth::UserId, DurableRepository, DurableStateKind, RepositoryFuture};

pub type TerminalSessionPk = i64;

/// Durable terminal lifecycle state.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TerminalSessionStateRecord {
    Created,
    Starting,
    Active,
    Detached,
    Reattaching,
    Terminating,
    Terminated,
    Failed,
    Expired,
}

/// Durable terminal session metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TerminalSessionRecord {
    pub id: TerminalSessionPk,
    pub session_id: String,
    pub user_id: UserId,
    pub node_id: Option<String>,
    pub state: TerminalSessionStateRecord,
    pub started_at_unix_secs: Option<i64>,
    pub ended_at_unix_secs: Option<i64>,
    pub exit_code: Option<i32>,
    pub created_at_unix_secs: i64,
}

/// Input for creating durable terminal session metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TerminalSessionInput {
    pub session_id: String,
    pub user_id: UserId,
    pub node_id: Option<String>,
    pub state: TerminalSessionStateRecord,
    pub started_at_unix_secs: Option<i64>,
}

/// Partial update for terminal session metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TerminalSessionUpdate {
    pub state: TerminalSessionStateRecord,
    pub ended_at_unix_secs: Option<i64>,
    pub exit_code: Option<i32>,
}

/// Repository boundary for durable terminal session metadata.
pub trait TerminalSessionMetadataRepository: DurableRepository {
    /// Creates terminal session metadata before live handles are attached.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot persist the metadata.
    fn create_terminal_session(
        &self,
        input: TerminalSessionInput,
    ) -> RepositoryFuture<'_, TerminalSessionRecord>;

    /// Updates terminal session lifecycle metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot update the metadata.
    fn update_terminal_session<'a>(
        &'a self,
        session_id: &'a str,
        update: TerminalSessionUpdate,
    ) -> RepositoryFuture<'a, TerminalSessionRecord>;

    /// Finds terminal session metadata by stable session ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot complete the lookup.
    fn find_terminal_session<'a>(
        &'a self,
        session_id: &'a str,
    ) -> RepositoryFuture<'a, Option<TerminalSessionRecord>>;

    /// Lists active or detached terminal metadata for a user.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot list sessions.
    fn list_reconnectable_sessions_for_user(
        &self,
        user_id: UserId,
    ) -> RepositoryFuture<'_, Vec<TerminalSessionRecord>>;
}

/// Marker repository for durable terminal session metadata.
pub struct TerminalSessionMetadataRepositoryBoundary;

impl DurableRepository for TerminalSessionMetadataRepositoryBoundary {
    const STATE_KIND: DurableStateKind = DurableStateKind::TerminalSessionMetadata;
}

#[cfg(test)]
mod tests {
    use super::TerminalSessionMetadataRepositoryBoundary;
    use crate::repositories::{DurableRepository, DurableStateKind};

    #[test]
    fn terminal_repository_marker_maps_to_durable_state() {
        assert_eq!(
            TerminalSessionMetadataRepositoryBoundary.state_kind(),
            DurableStateKind::TerminalSessionMetadata
        );
    }
}
