use crate::repositories::{DurableRepository, DurableStateKind, RepositoryFuture};

pub type UserId = i64;
pub type AuthSessionId = i64;
pub type MfaFactorId = i64;
pub type WorkspaceId = i64;
pub type RoleId = i64;

/// User role persisted with user identity state.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum UserRoleRecord {
    Admin,
    Operator,
    Viewer,
}

/// Durable user record read from the user repository.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UserRecord {
    pub id: UserId,
    pub email: String,
    pub password_hash: String,
    pub role: UserRoleRecord,
    pub created_at_unix_secs: i64,
    pub updated_at_unix_secs: i64,
}

/// Input for creating or updating durable user state.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UserInput {
    pub email: String,
    pub password_hash: String,
    pub role: UserRoleRecord,
}

/// Repository boundary for durable user identity state.
pub trait UserRepository: DurableRepository {
    /// Finds a user by primary key.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot complete the lookup.
    fn find_user_by_id(&self, user_id: UserId) -> RepositoryFuture<'_, Option<UserRecord>>;

    /// Finds a user by normalized email address.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot complete the lookup.
    fn find_user_by_email<'a>(&'a self, email: &'a str)
        -> RepositoryFuture<'a, Option<UserRecord>>;

    /// Creates or updates a durable user record.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot persist the user.
    fn upsert_user(&self, input: UserInput) -> RepositoryFuture<'_, UserRecord>;
}

/// Durable authentication session record.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AuthSessionRecord {
    pub id: AuthSessionId,
    pub user_id: UserId,
    pub token_hash: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub last_seen_at_unix_secs: Option<i64>,
    pub expires_at_unix_secs: Option<i64>,
    pub created_at_unix_secs: i64,
}

/// Input for creating a durable authentication session.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AuthSessionInput {
    pub user_id: UserId,
    pub token_hash: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub expires_at_unix_secs: Option<i64>,
}

/// Repository boundary for production-grade authentication sessions.
pub trait AuthSessionRepository: DurableRepository {
    /// Finds a session by the server-side token hash.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot complete the lookup.
    fn find_session_by_token_hash<'a>(
        &'a self,
        token_hash: &'a str,
    ) -> RepositoryFuture<'a, Option<AuthSessionRecord>>;

    /// Creates a durable authentication session.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot persist the session.
    fn create_session(&self, input: AuthSessionInput) -> RepositoryFuture<'_, AuthSessionRecord>;

    /// Deletes a session by token hash.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot delete the session.
    fn delete_session_by_token_hash<'a>(&'a self, token_hash: &'a str) -> RepositoryFuture<'a, ()>;
}

/// MFA factor kind persisted for a user.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MfaFactorKind {
    Password,
    Totp,
    RecoveryCode,
    WebAuthn,
    EmailOtp,
    HardwareKey,
    AdminApproval,
    SshKeySignature,
}

/// Purpose for recent MFA verification state.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MfaPurposeKind {
    Login,
    TerminalStepUp,
    FactorEnrollment,
}

/// Durable MFA factor metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MfaFactorRecord {
    pub id: MfaFactorId,
    pub user_id: UserId,
    pub factor_type: MfaFactorKind,
    pub label: String,
    pub enabled: bool,
    pub created_at_unix_secs: i64,
    pub updated_at_unix_secs: i64,
}

/// Input for creating or updating MFA factor metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MfaFactorInput {
    pub user_id: UserId,
    pub factor_type: MfaFactorKind,
    pub label: String,
    pub enabled: bool,
}

/// Input for durable recent MFA verification state.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MfaRecentVerificationInput {
    pub user_id: UserId,
    pub purpose: MfaPurposeKind,
    pub verified_at_unix_secs: i64,
    pub expires_at_unix_secs: i64,
}

/// Repository boundary for durable MFA factor and recent-verification state.
pub trait MfaFactorRepository: DurableRepository {
    /// Lists all factors registered for a user.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot list factors.
    fn list_factors_for_user(&self, user_id: UserId) -> RepositoryFuture<'_, Vec<MfaFactorRecord>>;

    /// Creates or updates factor metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot persist the factor.
    fn upsert_factor(&self, input: MfaFactorInput) -> RepositoryFuture<'_, MfaFactorRecord>;

    /// Records recent MFA verification state for step-up policy checks.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot persist the verification state.
    fn record_recent_verification(
        &self,
        input: MfaRecentVerificationInput,
    ) -> RepositoryFuture<'_, ()>;
}

/// Durable workspace record.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkspaceRecord {
    pub id: WorkspaceId,
    pub name: String,
    pub created_at_unix_secs: i64,
}

/// Durable role record.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RoleRecord {
    pub id: RoleId,
    pub name: String,
    pub created_at_unix_secs: i64,
}

/// Durable role permission assignment.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RolePermissionRecord {
    pub role_id: RoleId,
    pub permission: String,
}

/// Durable workspace membership record.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkspaceMemberRecord {
    pub workspace_id: WorkspaceId,
    pub user_id: UserId,
    pub role_id: RoleId,
}

/// Durable node-to-workspace assignment.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkspaceNodeRecord {
    pub workspace_id: WorkspaceId,
    pub node_id: String,
}

/// Repository boundary for RBAC, workspace membership, and node scoping.
pub trait RbacRepository: DurableRepository {
    /// Lists permissions for a user in a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot complete the lookup.
    fn permissions_for_user_in_workspace(
        &self,
        user_id: UserId,
        workspace_id: WorkspaceId,
    ) -> RepositoryFuture<'_, Vec<String>>;

    /// Finds the workspace assignment for a node.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot complete the lookup.
    fn workspace_for_node<'a>(
        &'a self,
        node_id: &'a str,
    ) -> RepositoryFuture<'a, Option<WorkspaceNodeRecord>>;

    /// Creates or updates a workspace membership.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot persist the membership.
    fn upsert_workspace_member(
        &self,
        member: WorkspaceMemberRecord,
    ) -> RepositoryFuture<'_, WorkspaceMemberRecord>;
}

/// Marker repository for durable user identity state.
pub struct UserRepositoryBoundary;

impl DurableRepository for UserRepositoryBoundary {
    const STATE_KIND: DurableStateKind = DurableStateKind::Users;
}

/// Marker repository for durable authentication session state.
pub struct AuthSessionRepositoryBoundary;

impl DurableRepository for AuthSessionRepositoryBoundary {
    const STATE_KIND: DurableStateKind = DurableStateKind::AuthSessions;
}

/// Marker repository for durable MFA state.
pub struct MfaFactorRepositoryBoundary;

impl DurableRepository for MfaFactorRepositoryBoundary {
    const STATE_KIND: DurableStateKind = DurableStateKind::MfaFactors;
}

/// Marker repository for durable RBAC and workspace state.
pub struct RbacRepositoryBoundary;

impl DurableRepository for RbacRepositoryBoundary {
    const STATE_KIND: DurableStateKind = DurableStateKind::Rbac;
}

/// Marker repository for durable workspace membership state.
pub struct WorkspaceMembershipRepositoryBoundary;

impl DurableRepository for WorkspaceMembershipRepositoryBoundary {
    const STATE_KIND: DurableStateKind = DurableStateKind::WorkspaceMemberships;
}

#[cfg(test)]
mod tests {
    use std::future;

    use super::{
        AuthSessionRepositoryBoundary, MfaFactorRepositoryBoundary, RbacRepositoryBoundary,
        UserInput, UserRecord, UserRepository, UserRepositoryBoundary, UserRoleRecord,
        WorkspaceMembershipRepositoryBoundary,
    };
    use crate::repositories::{DurableRepository, DurableStateKind, RepositoryFuture};

    struct MockUserRepository;

    impl DurableRepository for MockUserRepository {
        const STATE_KIND: DurableStateKind = DurableStateKind::Users;
    }

    impl UserRepository for MockUserRepository {
        fn find_user_by_id(&self, user_id: i64) -> RepositoryFuture<'_, Option<UserRecord>> {
            Box::pin(future::ready(Ok(Some(UserRecord {
                id: user_id,
                email: "admin@sunbolt.local".to_owned(),
                password_hash: "hash".to_owned(),
                role: UserRoleRecord::Admin,
                created_at_unix_secs: 1,
                updated_at_unix_secs: 2,
            }))))
        }

        fn find_user_by_email<'a>(
            &'a self,
            _email: &'a str,
        ) -> RepositoryFuture<'a, Option<UserRecord>> {
            Box::pin(future::ready(Ok(None)))
        }

        fn upsert_user(&self, input: UserInput) -> RepositoryFuture<'_, UserRecord> {
            Box::pin(future::ready(Ok(UserRecord {
                id: 1,
                email: input.email,
                password_hash: input.password_hash,
                role: input.role,
                created_at_unix_secs: 1,
                updated_at_unix_secs: 1,
            })))
        }
    }

    #[test]
    fn auth_repository_markers_map_to_durable_state() {
        assert_eq!(UserRepositoryBoundary.state_kind(), DurableStateKind::Users);
        assert_eq!(
            AuthSessionRepositoryBoundary.state_kind(),
            DurableStateKind::AuthSessions
        );
        assert_eq!(
            MfaFactorRepositoryBoundary.state_kind(),
            DurableStateKind::MfaFactors
        );
        assert_eq!(RbacRepositoryBoundary.state_kind(), DurableStateKind::Rbac);
        assert_eq!(
            WorkspaceMembershipRepositoryBoundary.state_kind(),
            DurableStateKind::WorkspaceMemberships
        );
    }

    #[tokio::test]
    async fn user_repository_boundary_can_be_mocked() {
        let repo = MockUserRepository;
        let user = repo
            .find_user_by_id(7)
            .await
            .expect("mock lookup succeeds")
            .expect("mock returns a user");

        assert_eq!(user.id, 7);
        assert_eq!(repo.state_kind(), DurableStateKind::Users);
    }
}
