pub mod audit;
pub mod auth;
pub mod node;
pub mod terminal;

pub use audit::{
    AuditEventInput, AuditEventRecord, AuditEventRepository, AuditEventRepositoryBoundary,
    AuditEventTarget,
};
pub use auth::{
    AuthSessionInput, AuthSessionRecord, AuthSessionRepository, AuthSessionRepositoryBoundary,
    MfaFactorInput, MfaFactorKind, MfaFactorRecord, MfaFactorRepository,
    MfaFactorRepositoryBoundary, MfaPurposeKind, MfaRecentVerificationInput, RbacRepository,
    RbacRepositoryBoundary, RolePermissionRecord, RoleRecord, UserInput, UserRecord,
    UserRepository, UserRepositoryBoundary, UserRoleRecord, WorkspaceMemberRecord,
    WorkspaceMembershipRepositoryBoundary, WorkspaceNodeRecord, WorkspaceRecord,
};
pub use node::{
    NodeCredentialInput, NodeCredentialRecord, NodeCredentialRepository,
    NodeCredentialRepositoryBoundary, NodeHeartbeatInput, NodeHeartbeatRecord,
    NodeHeartbeatRepository, NodeHeartbeatRepositoryBoundary, NodeInput, NodeRecord,
    NodeRepository, NodeRepositoryBoundary, NodeStatusRecord,
};
pub use terminal::{
    TerminalSessionInput, TerminalSessionMetadataRepository,
    TerminalSessionMetadataRepositoryBoundary, TerminalSessionRecord, TerminalSessionStateRecord,
    TerminalSessionUpdate,
};

use std::{future::Future, pin::Pin};

use crate::StorageError;

/// Future returned by storage repository contracts.
pub type RepositoryFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, StorageError>> + Send + 'a>>;

/// Durable production state categories owned by storage-backed repositories.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DurableStateKind {
    Users,
    AuthSessions,
    MfaFactors,
    Rbac,
    WorkspaceMemberships,
    Nodes,
    NodeCredentials,
    NodeHeartbeats,
    TerminalSessionMetadata,
    AuditEvents,
}

impl DurableStateKind {
    /// Returns the stable storage-boundary name used in diagnostics and tests.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Users => "users",
            Self::AuthSessions => "auth_sessions",
            Self::MfaFactors => "mfa_factors",
            Self::Rbac => "rbac",
            Self::WorkspaceMemberships => "workspace_memberships",
            Self::Nodes => "nodes",
            Self::NodeCredentials => "node_credentials",
            Self::NodeHeartbeats => "node_heartbeats",
            Self::TerminalSessionMetadata => "terminal_session_metadata",
            Self::AuditEvents => "audit_events",
        }
    }
}

/// Common marker for repository contracts that own durable production state.
pub trait DurableRepository {
    const STATE_KIND: DurableStateKind;

    /// Returns the durable state category handled by this repository.
    #[must_use]
    fn state_kind(&self) -> DurableStateKind {
        Self::STATE_KIND
    }
}

/// Runtime-only handle categories that must stay out of durable storage.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RuntimeHandleKind {
    LiveBrowserSocket,
    LiveAgentConnection,
    LivePtyHandle,
    TerminalOutputBroadcast,
    ShortReplayBuffer,
}

impl RuntimeHandleKind {
    /// Returns false because runtime handles are never the durable source of truth.
    #[must_use]
    pub const fn is_durable_source_of_truth(self) -> bool {
        false
    }

    /// Returns the stable runtime-handle name used in diagnostics and tests.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LiveBrowserSocket => "live_browser_socket",
            Self::LiveAgentConnection => "live_agent_connection",
            Self::LivePtyHandle => "live_pty_handle",
            Self::TerminalOutputBroadcast => "terminal_output_broadcast",
            Self::ShortReplayBuffer => "short_replay_buffer",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DurableStateKind, RuntimeHandleKind};

    #[test]
    fn phase_8_3_durable_boundaries_are_named() {
        let durable_boundaries = [
            DurableStateKind::Users,
            DurableStateKind::AuthSessions,
            DurableStateKind::MfaFactors,
            DurableStateKind::Rbac,
            DurableStateKind::WorkspaceMemberships,
            DurableStateKind::Nodes,
            DurableStateKind::NodeCredentials,
            DurableStateKind::NodeHeartbeats,
            DurableStateKind::TerminalSessionMetadata,
            DurableStateKind::AuditEvents,
        ];

        assert_eq!(durable_boundaries.len(), 10);
        assert_eq!(DurableStateKind::Users.as_str(), "users");
        assert_eq!(
            DurableStateKind::TerminalSessionMetadata.as_str(),
            "terminal_session_metadata"
        );
        assert_eq!(DurableStateKind::AuditEvents.as_str(), "audit_events");
    }

    #[test]
    fn runtime_handles_are_not_durable_sources_of_truth() {
        let runtime_handles = [
            RuntimeHandleKind::LiveBrowserSocket,
            RuntimeHandleKind::LiveAgentConnection,
            RuntimeHandleKind::LivePtyHandle,
            RuntimeHandleKind::TerminalOutputBroadcast,
            RuntimeHandleKind::ShortReplayBuffer,
        ];

        for handle in runtime_handles {
            assert!(!handle.is_durable_source_of_truth(), "{}", handle.as_str());
        }
    }
}
