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

pub use mfa::{
    AuthContext, AuthFactor, AuthFactorEnrollment, FactorChallenge, FactorResponse, FactorResult,
    FactorType, MfaPurpose,
};

const DEFAULT_SESSION_TTL_SECS: u64 = 8 * 60 * 60;
const DEFAULT_ADMIN_EMAIL: &str = "admin@sunbolt.local";
const DEFAULT_ADMIN_PASSWORD: &str = "sunbolt-dev-admin";

/// Session cookie used by Sunbolt HTTP and WebSocket authentication.
pub const SESSION_COOKIE_NAME: &str = "sunbolt_session";

/// Authentication and authorization service configuration.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AuthConfig {
    pub session_ttl: Duration,
    pub secure_cookie: bool,
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
        let secure_cookie = env_bool("SUNBOLT_COOKIE_SECURE").unwrap_or(false);
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
            secure_cookie,
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
}

#[derive(Debug, Default)]
struct AuthStore {
    users_by_id: HashMap<u64, StoredUser>,
    user_ids_by_email: HashMap<String, u64>,
    sessions_by_token: HashMap<String, SessionRecord>,
    auth_factors_by_id: HashMap<u64, AuthFactorEnrollment>,
    factor_ids_by_user_id: HashMap<u64, Vec<u64>>,
}

/// In-memory authentication service used for Phase 2 development flow.
#[derive(Debug, Clone)]
pub struct AuthService {
    store: Arc<Mutex<AuthStore>>,
    next_user_id: Arc<AtomicU64>,
    next_factor_id: Arc<AtomicU64>,
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
}

/// Permission identifiers are resource-oriented strings.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Permission(&'static str);

impl Permission {
    /// Permission required to open a terminal.
    pub const TERMINAL_OPEN: Self = Self("terminal.open");

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
    #[error("internal auth store is unavailable: {0}")]
    StoreUnavailable(&'static str),
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
        FactorResponse, FactorResult, FactorType, MfaPurpose, Permission, UserRole,
        DEFAULT_SESSION_TTL_SECS,
    };
    use std::time::Duration;

    #[test]
    fn bootstrap_admin_can_login() {
        let config = AuthConfig {
            session_ttl: Duration::from_secs(DEFAULT_SESSION_TTL_SECS),
            secure_cookie: false,
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
            secure_cookie: false,
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
    fn role_checks_gate_terminal_access() {
        let config = AuthConfig {
            session_ttl: Duration::from_secs(DEFAULT_SESSION_TTL_SECS),
            secure_cookie: false,
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

    fn test_auth_service() -> AuthService {
        AuthService::new(AuthConfig {
            session_ttl: Duration::from_secs(DEFAULT_SESSION_TTL_SECS),
            secure_cookie: false,
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
}
