use std::{
    collections::HashMap,
    env,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

mod mfa;
mod passkeys;
mod rbac;
mod recovery_codes;
mod totp;

pub use mfa::{
    AuthContext, AuthFactor, AuthFactorEnrollment, FactorChallenge, FactorResponse, FactorResult,
    FactorType, MfaPurpose,
};
pub use passkeys::{
    recommended_webauthn_crate, PasskeyAuthenticationChallenge, PasskeyCredential,
    PasskeyRegistrationChallenge, WebAuthnCrateChoice,
};
pub use rbac::{Role, RolePermission, Workspace, WorkspaceMember, WorkspaceNode};
pub use recovery_codes::RecoveryCodeBatch;
pub use totp::{TotpConfig, TotpEnrollment, TotpFactor, TotpRecoveryPath, TotpSecret};

const DEFAULT_SESSION_TTL_SECS: u64 = 8 * 60 * 60;
const DEFAULT_RECENT_MFA_TTL_SECS: u64 = 10 * 60;
const DEFAULT_ADMIN_EMAIL: &str = "admin@sunbolt.local";
const DEFAULT_ADMIN_PASSWORD: &str = "sunbolt-dev-admin";

/// Session cookie used by Sunbolt HTTP and WebSocket authentication.
pub const SESSION_COOKIE_NAME: &str = "sunbolt_session";

/// Authentication and authorization service configuration.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AuthConfig {
    pub session_ttl: Duration,
    pub recent_mfa_ttl: Duration,
    pub secure_cookie: bool,
    pub require_step_up_mfa_for_terminal: bool,
    pub bootstrap_admin: bool,
    pub admin_email: String,
    pub admin_password: String,
}

impl AuthConfig {
    /// Loads authentication configuration from environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        let session_ttl = Duration::from_secs(
            env_u64("SUNBOLT_SESSION_TTL_SECS").unwrap_or(DEFAULT_SESSION_TTL_SECS),
        );
        let recent_mfa_ttl = Duration::from_secs(
            env_u64("SUNBOLT_RECENT_MFA_TTL_SECS").unwrap_or(DEFAULT_RECENT_MFA_TTL_SECS),
        );
        let secure_cookie = env_bool("SUNBOLT_COOKIE_SECURE").unwrap_or(false);
        let require_step_up_mfa_for_terminal =
            env_bool("SUNBOLT_REQUIRE_TERMINAL_STEP_UP_MFA").unwrap_or(true);
        let bootstrap_admin = env_bool("SUNBOLT_DEV_BOOTSTRAP_ADMIN").unwrap_or(true);
        let admin_email = env::var("SUNBOLT_DEV_ADMIN_EMAIL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_ADMIN_EMAIL.to_owned());
        let admin_password = env::var("SUNBOLT_DEV_ADMIN_PASSWORD")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_ADMIN_PASSWORD.to_owned());

        Self {
            session_ttl,
            recent_mfa_ttl,
            secure_cookie,
            require_step_up_mfa_for_terminal,
            bootstrap_admin,
            admin_email,
            admin_password,
        }
    }
}

/// Simple user role model for MVP authorization.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    Operator,
    Viewer,
}

/// User model returned by auth APIs.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub email: String,
    pub role: UserRole,
}

#[derive(Debug, Clone)]
struct StoredUser {
    user: User,
    password_hash: String,
}

#[derive(Debug, Clone)]
struct SessionRecord {
    user_id: u64,
    expires_at: Instant,
    recent_mfa_at: Option<Instant>,
}

#[derive(Debug, Default)]
struct AuthStore {
    users_by_id: HashMap<u64, StoredUser>,
    user_ids_by_email: HashMap<String, u64>,
    sessions_by_token: HashMap<String, SessionRecord>,
    auth_factors_by_id: HashMap<u64, AuthFactorEnrollment>,
    factor_ids_by_user_id: HashMap<u64, Vec<u64>>,
    passkey_authentication_challenges_by_id: HashMap<String, PasskeyAuthenticationChallenge>,
    passkey_credentials_by_id: HashMap<u64, PasskeyCredential>,
    passkey_credential_ids_by_user_id: HashMap<u64, Vec<u64>>,
    passkey_registration_challenges_by_id: HashMap<String, PasskeyRegistrationChallenge>,
    recovery_codes_by_factor_id: HashMap<u64, Vec<recovery_codes::StoredRecoveryCode>>,
    role_permissions_by_role_id: HashMap<u64, Vec<RolePermission>>,
    roles_by_id: HashMap<u64, Role>,
    totp_secrets_by_factor_id: HashMap<u64, TotpSecret>,
    workspace_members_by_workspace_id: HashMap<u64, Vec<WorkspaceMember>>,
    workspace_nodes_by_node_id: HashMap<String, WorkspaceNode>,
    workspaces_by_id: HashMap<u64, Workspace>,
}

/// In-memory authentication service used for Phase 2 development flow.
#[derive(Debug, Clone)]
pub struct AuthService {
    store: Arc<Mutex<AuthStore>>,
    next_user_id: Arc<AtomicU64>,
    next_factor_id: Arc<AtomicU64>,
    next_passkey_credential_id: Arc<AtomicU64>,
    next_role_id: Arc<AtomicU64>,
    next_workspace_id: Arc<AtomicU64>,
    config: AuthConfig,
}

impl AuthService {
    /// Creates an auth service and optionally bootstraps a dev admin account.
    #[must_use]
    pub fn from_env() -> Self {
        Self::new(AuthConfig::from_env())
    }

    /// Creates an auth service from an explicit configuration.
    #[must_use]
    pub fn new(config: AuthConfig) -> Self {
        let service = Self {
            store: Arc::new(Mutex::new(AuthStore::default())),
            next_user_id: Arc::new(AtomicU64::new(1)),
            next_factor_id: Arc::new(AtomicU64::new(1)),
            next_passkey_credential_id: Arc::new(AtomicU64::new(1)),
            next_role_id: Arc::new(AtomicU64::new(1)),
            next_workspace_id: Arc::new(AtomicU64::new(1)),
            config,
        };
        if service.config.bootstrap_admin {
            let _ = service.bootstrap_admin();
        }
        service
    }

    /// Creates or updates the development bootstrap admin account.
    ///
    /// # Errors
    ///
    /// Returns an error when the auth store is unavailable or the configured
    /// bootstrap credentials are invalid.
    pub fn bootstrap_admin(&self) -> Result<User, AuthError> {
        self.upsert_user(
            &self.config.admin_email,
            &self.config.admin_password,
            UserRole::Admin,
        )
    }

    /// Adds or updates a user account.
    ///
    /// # Errors
    ///
    /// Returns an error when credentials are invalid or the auth store cannot
    /// be accessed.
    pub fn upsert_user(
        &self,
        email: &str,
        password: &str,
        role: UserRole,
    ) -> Result<User, AuthError> {
        if email.trim().is_empty() {
            return Err(AuthError::InvalidCredentials);
        }
        if password.is_empty() {
            return Err(AuthError::InvalidCredentials);
        }

        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("users"))?;
        let normalized_email = normalize_email(email);
        let password_hash = password_hash(password);

        if let Some(user_id) = store.user_ids_by_email.get(&normalized_email).copied() {
            let stored_user = store
                .users_by_id
                .get_mut(&user_id)
                .ok_or(AuthError::StoreUnavailable("user-record"))?;
            stored_user.user.role = role;
            stored_user.password_hash = password_hash;
            return Ok(stored_user.user.clone());
        }

        let user = User {
            id: self.next_user_id.fetch_add(1, Ordering::Relaxed),
            email: normalized_email.clone(),
            role,
        };
        store.user_ids_by_email.insert(normalized_email, user.id);
        store.users_by_id.insert(
            user.id,
            StoredUser {
                user: user.clone(),
                password_hash,
            },
        );
        Ok(user)
    }

    /// Authenticates a user and returns a session token.
    ///
    /// # Errors
    ///
    /// Returns an error when credentials are invalid or session state cannot
    /// be accessed.
    pub fn login(&self, email: &str, password: &str) -> Result<(User, String), AuthError> {
        let normalized_email = normalize_email(email);
        let now = Instant::now();
        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("sessions"))?;

        let Some(user_id) = store.user_ids_by_email.get(&normalized_email).copied() else {
            return Err(AuthError::InvalidCredentials);
        };
        let Some(stored_user) = store.users_by_id.get(&user_id) else {
            return Err(AuthError::InvalidCredentials);
        };
        let user = stored_user.user.clone();
        if stored_user.password_hash != password_hash(password) {
            return Err(AuthError::InvalidCredentials);
        }

        let token = random_token();
        store.sessions_by_token.insert(
            token.clone(),
            SessionRecord {
                user_id: user.id,
                expires_at: now + self.config.session_ttl,
                recent_mfa_at: None,
            },
        );
        Ok((user, token))
    }

    /// Returns the current user for a session token when valid.
    ///
    /// # Errors
    ///
    /// Returns an error when session state cannot be accessed.
    pub fn current_user(&self, token: &str) -> Result<Option<User>, AuthError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("sessions"))?;
        Self::purge_expired_sessions_locked(&mut store);

        let Some(session) = store.sessions_by_token.get(token) else {
            return Ok(None);
        };
        Ok(store
            .users_by_id
            .get(&session.user_id)
            .map(|user| user.user.clone()))
    }

    /// Removes an active session token.
    ///
    /// # Errors
    ///
    /// Returns an error when session state cannot be accessed.
    pub fn logout(&self, token: &str) -> Result<(), AuthError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("sessions"))?;
        store.sessions_by_token.remove(token);
        Ok(())
    }

    /// Returns true when a user can open terminal sessions.
    #[must_use]
    pub fn can_open_terminal(&self, user: &User) -> bool {
        matches!(user.role, UserRole::Admin | UserRole::Operator)
    }

    /// Returns true when terminal access requires recent step-up MFA.
    #[must_use]
    pub const fn terminal_step_up_policy_enabled(&self) -> bool {
        self.config.require_step_up_mfa_for_terminal
    }

    /// Returns true when a session has a recent MFA verification timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error when session state cannot be accessed.
    pub fn has_recent_mfa(&self, token: &str) -> Result<bool, AuthError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("sessions"))?;
        Self::purge_expired_sessions_locked(&mut store);
        let Some(session) = store.sessions_by_token.get(token) else {
            return Ok(false);
        };
        Ok(session
            .recent_mfa_at
            .is_some_and(|verified_at| verified_at.elapsed() <= self.config.recent_mfa_ttl))
    }

    /// Records a successful MFA verification on a session.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is missing or state cannot be
    /// accessed.
    pub fn record_mfa_success(&self, token: &str) -> Result<(), AuthError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("sessions"))?;
        Self::purge_expired_sessions_locked(&mut store);
        let Some(session) = store.sessions_by_token.get_mut(token) else {
            return Err(AuthError::InvalidSession);
        };
        session.recent_mfa_at = Some(Instant::now());
        Ok(())
    }

    /// Returns true when the user can open a terminal with this session.
    ///
    /// # Errors
    ///
    /// Returns an error when session state cannot be accessed.
    pub fn can_open_terminal_with_session(
        &self,
        user: &User,
        token: &str,
    ) -> Result<bool, AuthError> {
        if !self.can_open_terminal(user) {
            return Ok(false);
        }
        if !self.config.require_step_up_mfa_for_terminal {
            return Ok(true);
        }
        self.has_recent_mfa(token)
    }

    /// Enrolls MFA factor metadata for a user.
    ///
    /// # Errors
    ///
    /// Returns an error when the user does not exist, the label is empty, or
    /// auth state cannot be accessed.
    pub fn enroll_factor(
        &self,
        user_id: u64,
        factor_type: FactorType,
        label: &str,
    ) -> Result<AuthFactorEnrollment, AuthError> {
        if label.trim().is_empty() {
            return Err(AuthError::InvalidFactorLabel);
        }

        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("auth-factors"))?;
        if !store.users_by_id.contains_key(&user_id) {
            return Err(AuthError::UserNotFound);
        }

        let enrollment = AuthFactorEnrollment {
            id: self.next_factor_id.fetch_add(1, Ordering::Relaxed),
            user_id,
            factor_type,
            label: label.trim().to_owned(),
            enabled: true,
        };
        store
            .factor_ids_by_user_id
            .entry(user_id)
            .or_default()
            .push(enrollment.id);
        store
            .auth_factors_by_id
            .insert(enrollment.id, enrollment.clone());

        Ok(enrollment)
    }

    /// Enrolls a TOTP factor and returns the setup payload for QR display.
    ///
    /// # Errors
    ///
    /// Returns an error when factor enrollment fails.
    pub fn enroll_totp_factor(
        &self,
        user_id: u64,
        label: &str,
    ) -> Result<TotpEnrollment, AuthError> {
        let secret = TotpSecret::generate();
        self.enroll_totp_factor_with_secret(user_id, label, &secret)
    }

    /// Enrolls a TOTP factor with explicit secret material.
    ///
    /// # Errors
    ///
    /// Returns an error when factor enrollment fails or user state is missing.
    pub fn enroll_totp_factor_with_secret(
        &self,
        user_id: u64,
        label: &str,
        secret: &TotpSecret,
    ) -> Result<TotpEnrollment, AuthError> {
        let factor = self.enroll_factor(user_id, FactorType::Totp, label)?;
        let user_email = {
            let mut store = self
                .store
                .lock()
                .map_err(|_| AuthError::StoreUnavailable("totp"))?;
            store
                .totp_secrets_by_factor_id
                .insert(factor.id, secret.clone());
            store
                .users_by_id
                .get(&user_id)
                .ok_or(AuthError::UserNotFound)?
                .user
                .email
                .clone()
        };

        Ok(totp::build_totp_enrollment(
            factor,
            &user_email,
            &TotpConfig::default(),
            secret,
        ))
    }

    /// Generates and stores recovery codes for a user.
    ///
    /// # Errors
    ///
    /// Returns an error when factor enrollment fails or auth state cannot be
    /// accessed.
    pub fn generate_recovery_codes(&self, user_id: u64) -> Result<RecoveryCodeBatch, AuthError> {
        let factor = self.enroll_factor(
            user_id,
            recovery_codes::recovery_code_factor_type(),
            recovery_codes::recovery_code_factor_label(),
        )?;
        let codes = recovery_codes::generate_recovery_codes();
        let records = recovery_codes::recovery_code_records(&codes);
        self.store_recovery_code_records(factor.id, records)?;
        Ok(recovery_codes::build_recovery_code_batch(factor, codes))
    }

    /// Replaces all recovery codes for a user with a fresh batch.
    ///
    /// # Errors
    ///
    /// Returns an error when the user does not exist or auth state cannot be
    /// accessed.
    pub fn regenerate_recovery_codes(&self, user_id: u64) -> Result<RecoveryCodeBatch, AuthError> {
        let factor = self
            .recovery_code_factor_for_user(user_id)?
            .unwrap_or_else(|| AuthFactorEnrollment {
                id: 0,
                user_id,
                factor_type: FactorType::RecoveryCode,
                label: recovery_codes::recovery_code_factor_label().to_owned(),
                enabled: true,
            });
        if factor.id == 0 {
            return self.generate_recovery_codes(user_id);
        }

        let codes = recovery_codes::generate_recovery_codes();
        let records = recovery_codes::recovery_code_records(&codes);
        self.store_recovery_code_records(factor.id, records)?;
        Ok(recovery_codes::build_recovery_code_batch(factor, codes))
    }

    /// Verifies and invalidates a one-time recovery code.
    ///
    /// # Errors
    ///
    /// Returns an error when the user has no recovery code factor, the code is
    /// invalid or already used, or auth state cannot be accessed.
    pub fn verify_recovery_code(&self, user: &User, code: &str) -> Result<FactorResult, AuthError> {
        let Some(factor) = self.recovery_code_factor_for_user(user.id)? else {
            return Err(AuthError::RecoveryCodeInvalid);
        };
        let code_hash = recovery_codes::hash_recovery_code(code);
        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("recovery-codes"))?;
        let Some(records) = store.recovery_codes_by_factor_id.get_mut(&factor.id) else {
            return Err(AuthError::RecoveryCodeInvalid);
        };
        let Some(record) = records
            .iter_mut()
            .find(|record| record.hash == code_hash && !record.used)
        else {
            return Err(AuthError::RecoveryCodeInvalid);
        };
        record.used = true;
        Ok(FactorResult {
            user_id: user.id,
            factor_type: FactorType::RecoveryCode,
            verified: true,
        })
    }

    /// Begins a passkey registration ceremony.
    ///
    /// # Errors
    ///
    /// Returns an error when the relying party settings are invalid or auth
    /// state cannot be accessed.
    pub fn begin_passkey_registration(
        &self,
        user: &User,
        relying_party_id: &str,
        origin: &str,
    ) -> Result<PasskeyRegistrationChallenge, AuthError> {
        validate_passkey_relying_party(relying_party_id, origin)?;
        let challenge = passkeys::registration_challenge(
            user.id,
            &user.email,
            relying_party_id,
            sunbolt_common::product_name(),
            origin,
        );
        self.store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("passkeys"))?
            .passkey_registration_challenges_by_id
            .insert(challenge.challenge_id.clone(), challenge.clone());
        Ok(challenge)
    }

    /// Stores passkey credential metadata after browser-side registration.
    ///
    /// # Errors
    ///
    /// Returns an error when the registration challenge is missing, the label
    /// is empty, or auth state cannot be accessed.
    pub fn register_passkey_credential(
        &self,
        user_id: u64,
        challenge_id: &str,
        credential_id: &str,
        public_key: &str,
        label: &str,
    ) -> Result<PasskeyCredential, AuthError> {
        if label.trim().is_empty()
            || credential_id.trim().is_empty()
            || public_key.trim().is_empty()
        {
            return Err(AuthError::InvalidPasskeyCredential);
        }

        let factor = self.ensure_passkey_factor(user_id)?;
        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("passkeys"))?;
        let Some(challenge) = store
            .passkey_registration_challenges_by_id
            .remove(challenge_id)
        else {
            return Err(AuthError::PasskeyChallengeNotFound);
        };
        if challenge.user_id != user_id {
            return Err(AuthError::PasskeyChallengeNotFound);
        }

        let credential = passkeys::credential(
            self.next_passkey_credential_id
                .fetch_add(1, Ordering::Relaxed),
            user_id,
            credential_id,
            public_key,
            label,
        );
        store
            .passkey_credential_ids_by_user_id
            .entry(user_id)
            .or_default()
            .push(credential.id);
        store
            .passkey_credentials_by_id
            .insert(credential.id, credential.clone());
        store.auth_factors_by_id.insert(factor.id, factor);
        Ok(credential)
    }

    /// Begins a passkey authentication ceremony for an enrolled user.
    ///
    /// # Errors
    ///
    /// Returns an error when the user has no passkeys, relying party settings
    /// are invalid, or auth state cannot be accessed.
    pub fn begin_passkey_authentication(
        &self,
        user: &User,
        relying_party_id: &str,
        origin: &str,
    ) -> Result<PasskeyAuthenticationChallenge, AuthError> {
        validate_passkey_relying_party(relying_party_id, origin)?;
        let allowed_credential_ids = self
            .passkeys_for_user(user.id)?
            .into_iter()
            .map(|credential| credential.credential_id)
            .collect::<Vec<_>>();
        if allowed_credential_ids.is_empty() {
            return Err(AuthError::PasskeyCredentialUnavailable);
        }

        let challenge = passkeys::authentication_challenge(
            user.id,
            relying_party_id,
            origin,
            allowed_credential_ids,
        );
        self.store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("passkeys"))?
            .passkey_authentication_challenges_by_id
            .insert(challenge.challenge_id.clone(), challenge.clone());
        Ok(challenge)
    }

    /// Returns enabled passkey credentials for a user.
    ///
    /// # Errors
    ///
    /// Returns an error when auth state cannot be accessed.
    pub fn passkeys_for_user(&self, user_id: u64) -> Result<Vec<PasskeyCredential>, AuthError> {
        let store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("passkeys"))?;
        Ok(store
            .passkey_credential_ids_by_user_id
            .get(&user_id)
            .into_iter()
            .flatten()
            .filter_map(|credential_id| store.passkey_credentials_by_id.get(credential_id))
            .filter(|credential| credential.enabled)
            .cloned()
            .collect())
    }

    /// Creates a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error when the name is empty or RBAC state cannot be
    /// accessed.
    pub fn create_workspace(&self, name: &str) -> Result<Workspace, AuthError> {
        if name.trim().is_empty() {
            return Err(AuthError::InvalidRbacRecord);
        }
        let workspace =
            rbac::workspace(self.next_workspace_id.fetch_add(1, Ordering::Relaxed), name);
        self.store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("rbac"))?
            .workspaces_by_id
            .insert(workspace.id, workspace.clone());
        Ok(workspace)
    }

    /// Creates a workspace role.
    ///
    /// # Errors
    ///
    /// Returns an error when the name is empty or RBAC state cannot be
    /// accessed.
    pub fn create_role(&self, name: &str) -> Result<Role, AuthError> {
        if name.trim().is_empty() {
            return Err(AuthError::InvalidRbacRecord);
        }
        let role = rbac::role(self.next_role_id.fetch_add(1, Ordering::Relaxed), name);
        self.store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("rbac"))?
            .roles_by_id
            .insert(role.id, role.clone());
        Ok(role)
    }

    /// Grants a permission to a role.
    ///
    /// # Errors
    ///
    /// Returns an error when the role does not exist, permission is empty, or
    /// RBAC state cannot be accessed.
    pub fn grant_role_permission(
        &self,
        role_id: u64,
        permission: Permission,
    ) -> Result<RolePermission, AuthError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("rbac"))?;
        if !store.roles_by_id.contains_key(&role_id) {
            return Err(AuthError::RbacRecordNotFound);
        }
        let role_permission = rbac::role_permission(role_id, permission.as_str());
        store
            .role_permissions_by_role_id
            .entry(role_id)
            .or_default()
            .push(role_permission.clone());
        Ok(role_permission)
    }

    /// Adds a user to a workspace with a role.
    ///
    /// # Errors
    ///
    /// Returns an error when any referenced record is missing or RBAC state
    /// cannot be accessed.
    pub fn add_workspace_member(
        &self,
        workspace_id: u64,
        user_id: u64,
        role_id: u64,
    ) -> Result<WorkspaceMember, AuthError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("rbac"))?;
        if !store.workspaces_by_id.contains_key(&workspace_id)
            || !store.users_by_id.contains_key(&user_id)
            || !store.roles_by_id.contains_key(&role_id)
        {
            return Err(AuthError::RbacRecordNotFound);
        }
        let member = rbac::workspace_member(workspace_id, user_id, role_id);
        store
            .workspace_members_by_workspace_id
            .entry(workspace_id)
            .or_default()
            .push(member.clone());
        Ok(member)
    }

    /// Maps a node identifier to a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error when the workspace is missing, node id is empty, or
    /// RBAC state cannot be accessed.
    pub fn map_node_to_workspace(
        &self,
        workspace_id: u64,
        node_id: &str,
    ) -> Result<WorkspaceNode, AuthError> {
        if node_id.trim().is_empty() {
            return Err(AuthError::InvalidRbacRecord);
        }
        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("rbac"))?;
        if !store.workspaces_by_id.contains_key(&workspace_id) {
            return Err(AuthError::RbacRecordNotFound);
        }
        let node = rbac::workspace_node(workspace_id, node_id);
        store
            .workspace_nodes_by_node_id
            .insert(node.node_id.clone(), node.clone());
        Ok(node)
    }

    /// Returns true when the user has a permission in a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error when RBAC state cannot be accessed.
    pub fn user_has_workspace_permission(
        &self,
        user: &User,
        workspace_id: u64,
        permission: Permission,
    ) -> Result<bool, AuthError> {
        if user.role == UserRole::Admin {
            return Ok(true);
        }
        let store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("rbac"))?;
        let has_permission = store
            .workspace_members_by_workspace_id
            .get(&workspace_id)
            .into_iter()
            .flatten()
            .filter(|member| member.user_id == user.id)
            .any(|member| {
                store
                    .role_permissions_by_role_id
                    .get(&member.role_id)
                    .into_iter()
                    .flatten()
                    .any(|role_permission| role_permission.permission == permission.as_str())
            });
        Ok(has_permission)
    }

    /// Returns true when the user has a permission on the node's workspace.
    ///
    /// # Errors
    ///
    /// Returns an error when RBAC state cannot be accessed.
    pub fn user_has_node_permission(
        &self,
        user: &User,
        node_id: &str,
        permission: Permission,
    ) -> Result<bool, AuthError> {
        if user.role == UserRole::Admin {
            return Ok(true);
        }
        let workspace_id = {
            let store = self
                .store
                .lock()
                .map_err(|_| AuthError::StoreUnavailable("rbac"))?;
            store
                .workspace_nodes_by_node_id
                .get(node_id)
                .map(|node| node.workspace_id)
        };
        let Some(workspace_id) = workspace_id else {
            return Ok(false);
        };
        self.user_has_workspace_permission(user, workspace_id, permission)
    }

    /// Returns enabled MFA factors for a user.
    ///
    /// # Errors
    ///
    /// Returns an error when auth state cannot be accessed.
    pub fn factors_for_user(&self, user_id: u64) -> Result<Vec<AuthFactorEnrollment>, AuthError> {
        let store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("auth-factors"))?;
        Ok(store
            .factor_ids_by_user_id
            .get(&user_id)
            .into_iter()
            .flatten()
            .filter_map(|factor_id| store.auth_factors_by_id.get(factor_id))
            .filter(|factor| factor.enabled)
            .cloned()
            .collect())
    }

    /// Begins a challenge for an enrolled MFA factor.
    ///
    /// # Errors
    ///
    /// Returns an error when the factor is not enrolled for the user, or the
    /// factor implementation cannot begin a challenge.
    pub fn begin_factor_challenge(
        &self,
        factor: &dyn AuthFactor,
        ctx: &AuthContext,
    ) -> Result<FactorChallenge, AuthError> {
        self.ensure_factor_enrolled(ctx.user.id, factor.factor_type())?;
        factor.begin_challenge(ctx)
    }

    /// Verifies a challenge response for an enrolled MFA factor.
    ///
    /// # Errors
    ///
    /// Returns an error when the factor is not enrolled for the user, the
    /// response is for a different factor type, or verification fails.
    pub fn verify_factor_challenge(
        &self,
        factor: &dyn AuthFactor,
        ctx: &AuthContext,
        response: FactorResponse,
    ) -> Result<FactorResult, AuthError> {
        if response.factor_type != factor.factor_type() {
            return Err(AuthError::FactorTypeMismatch);
        }
        self.ensure_factor_enrolled(ctx.user.id, factor.factor_type())?;
        let result = factor.verify_challenge(ctx, response)?;
        if result.user_id != ctx.user.id || result.factor_type != factor.factor_type() {
            return Err(AuthError::FactorTypeMismatch);
        }
        if !result.verified {
            return Err(AuthError::FactorVerificationFailed);
        }
        Ok(result)
    }

    /// Returns the configured TOTP factor for a user when enrolled.
    ///
    /// # Errors
    ///
    /// Returns an error when no enabled TOTP factor exists or auth state
    /// cannot be accessed.
    pub fn totp_factor_for_user(&self, user_id: u64) -> Result<TotpFactor, AuthError> {
        let store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("totp"))?;
        let Some(secret) = store
            .factor_ids_by_user_id
            .get(&user_id)
            .into_iter()
            .flatten()
            .find_map(|factor_id| {
                store
                    .auth_factors_by_id
                    .get(factor_id)
                    .filter(|factor| factor.enabled && factor.factor_type == FactorType::Totp)
                    .and_then(|factor| store.totp_secrets_by_factor_id.get(&factor.id))
            })
        else {
            return Err(AuthError::TotpSecretMissing);
        };

        Ok(TotpFactor::new(secret.clone(), TotpConfig::default()))
    }

    /// Verifies a TOTP code for a user through the generic MFA flow.
    ///
    /// # Errors
    ///
    /// Returns an error when the TOTP factor is not enrolled or verification
    /// fails.
    pub fn verify_totp_code(
        &self,
        user: &User,
        purpose: MfaPurpose,
        code: &str,
    ) -> Result<FactorResult, AuthError> {
        let factor = self.totp_factor_for_user(user.id)?;
        let ctx = AuthContext {
            user: user.clone(),
            purpose,
        };
        let challenge = self.begin_factor_challenge(&factor, &ctx)?;
        self.verify_factor_challenge(
            &factor,
            &ctx,
            FactorResponse {
                challenge_id: challenge.challenge_id,
                factor_type: FactorType::Totp,
                secret: code.to_owned(),
            },
        )
    }

    /// Returns recovery guidance for a TOTP-enrolled user.
    ///
    /// # Errors
    ///
    /// Returns an error when the user has no TOTP factor.
    pub fn totp_recovery_path(&self, user_id: u64) -> Result<TotpRecoveryPath, AuthError> {
        self.ensure_factor_enrolled(user_id, FactorType::Totp)?;
        Ok(totp::default_totp_recovery_path(user_id))
    }

    /// Builds a `Set-Cookie` header value for an authenticated session.
    #[must_use]
    pub fn session_cookie_header(&self, token: &str) -> String {
        let max_age = self.config.session_ttl.as_secs();
        let mut cookie = format!(
            "{SESSION_COOKIE_NAME}={token}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Strict"
        );
        if self.config.secure_cookie {
            cookie.push_str("; Secure");
        }
        cookie
    }

    /// Builds a `Set-Cookie` header value that clears the auth session.
    #[must_use]
    pub fn clear_session_cookie_header(&self) -> String {
        let mut cookie =
            format!("{SESSION_COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict");
        if self.config.secure_cookie {
            cookie.push_str("; Secure");
        }
        cookie
    }

    fn purge_expired_sessions_locked(store: &mut AuthStore) {
        let now = Instant::now();
        store
            .sessions_by_token
            .retain(|_, session| session.expires_at > now);
    }

    fn ensure_factor_enrolled(
        &self,
        user_id: u64,
        factor_type: FactorType,
    ) -> Result<(), AuthError> {
        let store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("auth-factors"))?;
        let enrolled = store
            .factor_ids_by_user_id
            .get(&user_id)
            .into_iter()
            .flatten()
            .filter_map(|factor_id| store.auth_factors_by_id.get(factor_id))
            .any(|factor| factor.enabled && factor.factor_type == factor_type);
        if enrolled {
            Ok(())
        } else {
            Err(AuthError::FactorNotEnrolled)
        }
    }

    fn recovery_code_factor_for_user(
        &self,
        user_id: u64,
    ) -> Result<Option<AuthFactorEnrollment>, AuthError> {
        let store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("recovery-codes"))?;
        Ok(store
            .factor_ids_by_user_id
            .get(&user_id)
            .into_iter()
            .flatten()
            .filter_map(|factor_id| store.auth_factors_by_id.get(factor_id))
            .find(|factor| factor.enabled && factor.factor_type == FactorType::RecoveryCode)
            .cloned())
    }

    fn store_recovery_code_records(
        &self,
        factor_id: u64,
        records: Vec<recovery_codes::StoredRecoveryCode>,
    ) -> Result<(), AuthError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("recovery-codes"))?;
        store.recovery_codes_by_factor_id.insert(factor_id, records);
        Ok(())
    }

    fn ensure_passkey_factor(&self, user_id: u64) -> Result<AuthFactorEnrollment, AuthError> {
        if let Some(factor) = self.passkey_factor_for_user(user_id)? {
            return Ok(factor);
        }
        self.enroll_factor(user_id, FactorType::WebAuthn, "Passkeys")
    }

    fn passkey_factor_for_user(
        &self,
        user_id: u64,
    ) -> Result<Option<AuthFactorEnrollment>, AuthError> {
        let store = self
            .store
            .lock()
            .map_err(|_| AuthError::StoreUnavailable("passkeys"))?;
        Ok(store
            .factor_ids_by_user_id
            .get(&user_id)
            .into_iter()
            .flatten()
            .filter_map(|factor_id| store.auth_factors_by_id.get(factor_id))
            .find(|factor| factor.enabled && factor.factor_type == FactorType::WebAuthn)
            .cloned())
    }
}

/// Permission identifiers are resource-oriented strings.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Permission(&'static str);

impl Permission {
    /// Permission required to open a terminal.
    pub const TERMINAL_OPEN: Self = Self("terminal.open");
    /// Permission required to reattach a terminal.
    pub const TERMINAL_REATTACH: Self = Self("terminal.reattach");
    /// Permission required to terminate a terminal.
    pub const TERMINAL_CLOSE: Self = Self("terminal.close");
    /// Permission required to view a node.
    pub const NODE_VIEW: Self = Self("node.view");
    /// Permission required to register a node.
    pub const NODE_REGISTER: Self = Self("node.register");
    /// Permission required to revoke a node.
    pub const NODE_REVOKE: Self = Self("node.revoke");
    /// Permission required to rotate node identity credentials.
    pub const NODE_CREDENTIAL_ROTATE: Self = Self("node.credential.rotate");
    /// Permission required to manage users.
    pub const USER_MANAGE: Self = Self("user.manage");
    /// Permission required to manage roles.
    pub const ROLE_MANAGE: Self = Self("role.manage");

    /// Returns the stable permission identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("invalid auth session")]
    InvalidSession,
    #[error("user was not found")]
    UserNotFound,
    #[error("invalid MFA factor label")]
    InvalidFactorLabel,
    #[error("MFA factor is not enrolled for this user")]
    FactorNotEnrolled,
    #[error("MFA factor response type does not match the challenge")]
    FactorTypeMismatch,
    #[error("MFA factor verification failed")]
    FactorVerificationFailed,
    #[error("TOTP secret is not available")]
    TotpSecretMissing,
    #[error("recovery code is invalid or already used")]
    RecoveryCodeInvalid,
    #[error("invalid passkey relying party settings")]
    InvalidPasskeyRelyingParty,
    #[error("invalid passkey credential metadata")]
    InvalidPasskeyCredential,
    #[error("passkey challenge was not found")]
    PasskeyChallengeNotFound,
    #[error("passkey credential is not available")]
    PasskeyCredentialUnavailable,
    #[error("invalid RBAC record")]
    InvalidRbacRecord,
    #[error("RBAC record was not found")]
    RbacRecordNotFound,
    #[error("internal auth store is unavailable: {0}")]
    StoreUnavailable(&'static str),
}

fn validate_passkey_relying_party(relying_party_id: &str, origin: &str) -> Result<(), AuthError> {
    if relying_party_id.trim().is_empty() || origin.trim().is_empty() {
        return Err(AuthError::InvalidPasskeyRelyingParty);
    }
    Ok(())
}

fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

fn password_hash(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"sunbolt-dev-v1:");
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn random_token() -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);

    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        token.push(char::from(HEX[usize::from(byte >> 4)]));
        token.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    token
}

fn env_u64(name: &str) -> Option<u64> {
    env::var(name).ok()?.parse().ok()
}

fn env_bool(name: &str) -> Option<bool> {
    let value = env::var(name).ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuthConfig, AuthContext, AuthError, AuthFactor, AuthService, FactorChallenge,
        FactorResponse, FactorResult, FactorType, MfaPurpose, Permission, TotpSecret, UserRole,
        DEFAULT_RECENT_MFA_TTL_SECS, DEFAULT_SESSION_TTL_SECS,
    };
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn bootstrap_admin_can_login() {
        let config = AuthConfig {
            session_ttl: Duration::from_secs(DEFAULT_SESSION_TTL_SECS),
            recent_mfa_ttl: Duration::from_secs(DEFAULT_RECENT_MFA_TTL_SECS),
            secure_cookie: false,
            require_step_up_mfa_for_terminal: true,
            bootstrap_admin: true,
            admin_email: "admin@example.com".to_owned(),
            admin_password: "password123".to_owned(),
        };
        let auth = AuthService::new(config);
        let (user, token) = auth
            .login("admin@example.com", "password123")
            .expect("admin login should work");

        assert_eq!(user.role, UserRole::Admin);
        assert_eq!(
            auth.current_user(&token)
                .expect("session should be readable")
                .expect("session should exist")
                .email,
            "admin@example.com"
        );
    }

    #[test]
    fn logout_invalidates_session() {
        let config = AuthConfig {
            session_ttl: Duration::from_secs(DEFAULT_SESSION_TTL_SECS),
            recent_mfa_ttl: Duration::from_secs(DEFAULT_RECENT_MFA_TTL_SECS),
            secure_cookie: false,
            require_step_up_mfa_for_terminal: true,
            bootstrap_admin: true,
            admin_email: "admin@example.com".to_owned(),
            admin_password: "password123".to_owned(),
        };
        let auth = AuthService::new(config);
        let (_, token) = auth
            .login("admin@example.com", "password123")
            .expect("admin login should work");

        auth.logout(&token).expect("logout should succeed");
        assert!(auth
            .current_user(&token)
            .expect("session should be readable")
            .is_none());
    }

    #[test]
    fn session_cookie_uses_http_only_strict_same_site_and_secure_when_enabled() {
        let auth = AuthService::new(AuthConfig {
            session_ttl: Duration::from_secs(DEFAULT_SESSION_TTL_SECS),
            recent_mfa_ttl: Duration::from_secs(DEFAULT_RECENT_MFA_TTL_SECS),
            secure_cookie: true,
            require_step_up_mfa_for_terminal: true,
            bootstrap_admin: false,
            admin_email: "ignore@example.com".to_owned(),
            admin_password: "ignore".to_owned(),
        });

        let cookie = auth.session_cookie_header("session-token");

        assert!(cookie.starts_with("sunbolt_session=session-token;"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Secure"));
    }

    #[test]
    fn clear_session_cookie_preserves_security_attributes() {
        let auth = AuthService::new(AuthConfig {
            session_ttl: Duration::from_secs(DEFAULT_SESSION_TTL_SECS),
            recent_mfa_ttl: Duration::from_secs(DEFAULT_RECENT_MFA_TTL_SECS),
            secure_cookie: true,
            require_step_up_mfa_for_terminal: true,
            bootstrap_admin: false,
            admin_email: "ignore@example.com".to_owned(),
            admin_password: "ignore".to_owned(),
        });

        let cookie = auth.clear_session_cookie_header();

        assert!(cookie.starts_with("sunbolt_session=;"));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Secure"));
    }

    #[test]
    fn role_checks_gate_terminal_access() {
        let config = AuthConfig {
            session_ttl: Duration::from_secs(DEFAULT_SESSION_TTL_SECS),
            recent_mfa_ttl: Duration::from_secs(DEFAULT_RECENT_MFA_TTL_SECS),
            secure_cookie: false,
            require_step_up_mfa_for_terminal: true,
            bootstrap_admin: false,
            admin_email: "ignore@example.com".to_owned(),
            admin_password: "ignore".to_owned(),
        };
        let auth = AuthService::new(config);
        let admin = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");
        let operator = auth
            .upsert_user("operator@example.com", "pass", UserRole::Operator)
            .expect("operator should upsert");
        let viewer = auth
            .upsert_user("viewer@example.com", "pass", UserRole::Viewer)
            .expect("viewer should upsert");

        assert!(auth.can_open_terminal(&admin));
        assert!(auth.can_open_terminal(&operator));
        assert!(!auth.can_open_terminal(&viewer));
        assert_eq!(Permission::TERMINAL_OPEN.as_str(), "terminal.open");
    }

    #[test]
    fn auth_service_enrolls_and_lists_mfa_factors() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");

        let enrollment = auth
            .enroll_factor(user.id, FactorType::Totp, "Authenticator app")
            .expect("factor should enroll");
        let factors = auth
            .factors_for_user(user.id)
            .expect("factors should be listed");

        assert_eq!(enrollment.id, 1);
        assert_eq!(enrollment.factor_type, FactorType::Totp);
        assert_eq!(factors, vec![enrollment]);
    }

    #[test]
    fn auth_service_rejects_factor_enrollment_for_unknown_user() {
        let auth = test_auth_service();
        let error = auth
            .enroll_factor(42, FactorType::Totp, "Authenticator app")
            .expect_err("unknown user should be rejected");

        assert!(matches!(error, AuthError::UserNotFound));
    }

    #[test]
    fn auth_service_runs_mfa_challenge_and_verification_flow() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");
        auth.enroll_factor(user.id, FactorType::Totp, "Authenticator app")
            .expect("factor should enroll");
        let ctx = AuthContext {
            user,
            purpose: MfaPurpose::TerminalStepUp,
        };
        let factor = StaticFactor {
            factor_type: FactorType::Totp,
            expected_secret: "123456",
        };

        let challenge = auth
            .begin_factor_challenge(&factor, &ctx)
            .expect("challenge should begin");
        let result = auth
            .verify_factor_challenge(
                &factor,
                &ctx,
                FactorResponse {
                    challenge_id: challenge.challenge_id,
                    factor_type: FactorType::Totp,
                    secret: "123456".to_owned(),
                },
            )
            .expect("factor should verify");

        assert!(result.verified);
    }

    #[test]
    fn auth_service_rejects_unenrolled_mfa_factor() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");
        let ctx = AuthContext {
            user,
            purpose: MfaPurpose::Login,
        };
        let factor = StaticFactor {
            factor_type: FactorType::Totp,
            expected_secret: "123456",
        };

        let error = auth
            .begin_factor_challenge(&factor, &ctx)
            .expect_err("unenrolled factor should be rejected");

        assert!(matches!(error, AuthError::FactorNotEnrolled));
    }

    #[test]
    fn auth_service_requires_recent_mfa_for_terminal_policy() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");
        let (_, token) = auth
            .login("admin@example.com", "pass")
            .expect("login should work");

        assert!(auth.terminal_step_up_policy_enabled());
        assert!(!auth
            .can_open_terminal_with_session(&user, &token)
            .expect("terminal check should read session"));

        auth.record_mfa_success(&token)
            .expect("MFA success should record");
        assert!(auth
            .can_open_terminal_with_session(&user, &token)
            .expect("terminal check should read session"));
    }

    #[test]
    fn auth_service_enrolls_totp_with_qr_payload() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");

        let enrollment = auth
            .enroll_totp_factor_with_secret(
                user.id,
                "Authenticator app",
                &TotpSecret::from_bytes(b"sunbolt-secret".to_vec()),
            )
            .expect("TOTP should enroll");

        assert_eq!(enrollment.factor.factor_type, FactorType::Totp);
        assert!(enrollment.provisioning_uri.starts_with("otpauth://totp/"));
        assert_eq!(enrollment.qr_code_payload, enrollment.provisioning_uri);
        assert!(enrollment.secret_base32.len() >= 16);
    }

    #[test]
    fn auth_service_verifies_totp_code() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");
        auth.enroll_totp_factor_with_secret(
            user.id,
            "Authenticator app",
            &TotpSecret::from_bytes(b"sunbolt-secret".to_vec()),
        )
        .expect("TOTP should enroll");
        let factor = auth
            .totp_factor_for_user(user.id)
            .expect("TOTP factor should exist");
        let code = factor.code_at(now_unix_secs());

        let result = auth
            .verify_totp_code(&user, MfaPurpose::TerminalStepUp, &code)
            .expect("TOTP should verify");

        assert!(result.verified);
    }

    #[test]
    fn auth_service_exposes_totp_recovery_path() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");
        auth.enroll_totp_factor_with_secret(
            user.id,
            "Authenticator app",
            &TotpSecret::from_bytes(b"sunbolt-secret".to_vec()),
        )
        .expect("TOTP should enroll");

        let recovery = auth
            .totp_recovery_path(user.id)
            .expect("recovery path should be available");

        assert_eq!(recovery.user_id, user.id);
        assert!(recovery
            .fallback_factors
            .contains(&FactorType::RecoveryCode));
    }

    #[test]
    fn auth_service_generates_recovery_codes_once() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");

        let batch = auth
            .generate_recovery_codes(user.id)
            .expect("recovery codes should generate");
        let factors = auth
            .factors_for_user(user.id)
            .expect("factors should be listed");

        assert_eq!(batch.factor.factor_type, FactorType::RecoveryCode);
        assert_eq!(batch.codes.len(), 10);
        assert!(factors
            .iter()
            .any(|factor| factor.factor_type == FactorType::RecoveryCode));
    }

    #[test]
    fn auth_service_verifies_and_invalidates_recovery_code() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");
        let batch = auth
            .generate_recovery_codes(user.id)
            .expect("recovery codes should generate");
        let code = batch.codes[0].clone();

        let result = auth
            .verify_recovery_code(&user, &code)
            .expect("first recovery code use should verify");
        assert!(result.verified);
        assert_eq!(result.factor_type, FactorType::RecoveryCode);

        let error = auth
            .verify_recovery_code(&user, &code)
            .expect_err("second recovery code use should fail");
        assert!(matches!(error, AuthError::RecoveryCodeInvalid));
    }

    #[test]
    fn auth_service_regenerates_recovery_codes() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");
        let first_batch = auth
            .generate_recovery_codes(user.id)
            .expect("recovery codes should generate");
        let old_code = first_batch.codes[0].clone();
        let second_batch = auth
            .regenerate_recovery_codes(user.id)
            .expect("recovery codes should regenerate");

        assert_eq!(second_batch.factor.id, first_batch.factor.id);
        assert_eq!(second_batch.codes.len(), 10);
        assert_ne!(second_batch.codes, first_batch.codes);
        assert!(matches!(
            auth.verify_recovery_code(&user, &old_code),
            Err(AuthError::RecoveryCodeInvalid)
        ));
    }

    #[test]
    fn auth_service_records_webauthn_crate_choice() {
        let choice = super::recommended_webauthn_crate();

        assert_eq!(choice.crate_name, "webauthn-rs");
        assert!(choice.rationale.contains("Sunbolt"));
    }

    #[test]
    fn auth_service_begins_passkey_registration_challenge() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");

        let challenge = auth
            .begin_passkey_registration(&user, "localhost", "http://localhost:3000")
            .expect("passkey registration should begin");

        assert_eq!(challenge.user_id, user.id);
        assert_eq!(challenge.user_email, user.email);
        assert_eq!(challenge.relying_party_name, "Sunbolt");
        assert!(!challenge.challenge.is_empty());
    }

    #[test]
    fn auth_service_registers_and_lists_passkey_credentials() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");
        let challenge = auth
            .begin_passkey_registration(&user, "localhost", "http://localhost:3000")
            .expect("passkey registration should begin");

        let credential = auth
            .register_passkey_credential(
                user.id,
                &challenge.challenge_id,
                "credential-1",
                "public-key-1",
                "Laptop passkey",
            )
            .expect("passkey credential should register");
        let credentials = auth
            .passkeys_for_user(user.id)
            .expect("passkeys should list");
        let factors = auth.factors_for_user(user.id).expect("factors should list");

        assert_eq!(credential.credential_id, "credential-1");
        assert_eq!(credentials, vec![credential]);
        assert!(factors
            .iter()
            .any(|factor| factor.factor_type == FactorType::WebAuthn));
    }

    #[test]
    fn auth_service_begins_passkey_authentication_challenge() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");
        let registration = auth
            .begin_passkey_registration(&user, "localhost", "http://localhost:3000")
            .expect("passkey registration should begin");
        auth.register_passkey_credential(
            user.id,
            &registration.challenge_id,
            "credential-1",
            "public-key-1",
            "Laptop passkey",
        )
        .expect("passkey credential should register");

        let authentication = auth
            .begin_passkey_authentication(&user, "localhost", "http://localhost:3000")
            .expect("passkey authentication should begin");

        assert_eq!(authentication.user_id, user.id);
        assert_eq!(authentication.allowed_credential_ids, vec!["credential-1"]);
        assert!(!authentication.challenge.is_empty());
    }

    #[test]
    fn auth_service_rejects_passkey_authentication_without_credentials() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("admin@example.com", "pass", UserRole::Admin)
            .expect("admin should upsert");

        let error = auth
            .begin_passkey_authentication(&user, "localhost", "http://localhost:3000")
            .expect_err("missing credential should reject authentication");

        assert!(matches!(error, AuthError::PasskeyCredentialUnavailable));
    }

    #[test]
    fn auth_service_grants_workspace_permissions_through_roles() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("operator@example.com", "pass", UserRole::Operator)
            .expect("operator should upsert");
        let workspace = auth
            .create_workspace("Operations")
            .expect("workspace should be created");
        let role = auth
            .create_role("Operator")
            .expect("role should be created");
        auth.grant_role_permission(role.id, Permission::TERMINAL_OPEN)
            .expect("permission should be granted");
        auth.add_workspace_member(workspace.id, user.id, role.id)
            .expect("member should be added");

        assert!(auth
            .user_has_workspace_permission(&user, workspace.id, Permission::TERMINAL_OPEN)
            .expect("permission check should work"));
        assert!(!auth
            .user_has_workspace_permission(&user, workspace.id, Permission::NODE_REVOKE)
            .expect("permission check should work"));
    }

    #[test]
    fn auth_service_checks_node_permissions_through_workspace_mapping() {
        let auth = test_auth_service();
        let user = auth
            .upsert_user("operator@example.com", "pass", UserRole::Operator)
            .expect("operator should upsert");
        let workspace = auth
            .create_workspace("Operations")
            .expect("workspace should be created");
        let role = auth
            .create_role("Operator")
            .expect("role should be created");
        auth.grant_role_permission(role.id, Permission::NODE_VIEW)
            .expect("permission should be granted");
        auth.add_workspace_member(workspace.id, user.id, role.id)
            .expect("member should be added");
        auth.map_node_to_workspace(workspace.id, "node-1")
            .expect("node should map");

        assert!(auth
            .user_has_node_permission(&user, "node-1", Permission::NODE_VIEW)
            .expect("node permission check should work"));
        assert!(!auth
            .user_has_node_permission(&user, "node-2", Permission::NODE_VIEW)
            .expect("node permission check should work"));
    }

    fn test_auth_service() -> AuthService {
        AuthService::new(AuthConfig {
            session_ttl: Duration::from_secs(DEFAULT_SESSION_TTL_SECS),
            recent_mfa_ttl: Duration::from_secs(DEFAULT_RECENT_MFA_TTL_SECS),
            secure_cookie: false,
            require_step_up_mfa_for_terminal: true,
            bootstrap_admin: false,
            admin_email: "ignore@example.com".to_owned(),
            admin_password: "ignore".to_owned(),
        })
    }

    struct StaticFactor {
        factor_type: FactorType,
        expected_secret: &'static str,
    }

    impl AuthFactor for StaticFactor {
        fn factor_type(&self) -> FactorType {
            self.factor_type
        }

        fn begin_challenge(&self, ctx: &AuthContext) -> Result<FactorChallenge, AuthError> {
            Ok(FactorChallenge {
                challenge_id: "challenge-1".to_owned(),
                user_id: ctx.user.id,
                factor_type: self.factor_type,
                purpose: ctx.purpose,
                message: "Enter MFA code".to_owned(),
            })
        }

        fn verify_challenge(
            &self,
            ctx: &AuthContext,
            response: FactorResponse,
        ) -> Result<FactorResult, AuthError> {
            Ok(FactorResult {
                user_id: ctx.user.id,
                factor_type: response.factor_type,
                verified: response.secret == self.expected_secret,
            })
        }
    }

    fn now_unix_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}
