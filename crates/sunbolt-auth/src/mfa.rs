use serde::{Deserialize, Serialize};

use crate::{AuthError, User};

/// Extensible MFA factor kinds supported by Sunbolt policy.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactorType {
    Password,
    Totp,
    RecoveryCode,
    WebAuthn,
    EmailOtp,
    HardwareKey,
    AdminApproval,
    SshKeySignature,
}

/// Reason an MFA challenge is being requested.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MfaPurpose {
    Login,
    TerminalStepUp,
    FactorEnrollment,
}

/// Context passed to an MFA factor implementation.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AuthContext {
    pub user: User,
    pub purpose: MfaPurpose,
}

/// Registered MFA factor metadata for a user.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthFactorEnrollment {
    pub id: u64,
    pub user_id: u64,
    pub factor_type: FactorType,
    pub label: String,
    pub enabled: bool,
}

/// Challenge returned when a factor begins verification.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FactorChallenge {
    pub challenge_id: String,
    pub user_id: u64,
    pub factor_type: FactorType,
    pub purpose: MfaPurpose,
    pub message: String,
}

/// User response submitted for a factor challenge.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FactorResponse {
    pub challenge_id: String,
    pub factor_type: FactorType,
    pub secret: String,
}

/// Result of factor verification.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FactorResult {
    pub user_id: u64,
    pub factor_type: FactorType,
    pub verified: bool,
}

/// Common interface for MFA factor implementations.
pub trait AuthFactor: Send + Sync {
    fn factor_type(&self) -> FactorType;

    /// Begins a factor challenge.
    ///
    /// # Errors
    ///
    /// Returns an error when the factor cannot produce a challenge for the
    /// supplied context.
    fn begin_challenge(&self, ctx: &AuthContext) -> Result<FactorChallenge, AuthError>;

    /// Verifies a factor challenge response.
    ///
    /// # Errors
    ///
    /// Returns an error when the factor cannot verify the response.
    fn verify_challenge(
        &self,
        ctx: &AuthContext,
        response: FactorResponse,
    ) -> Result<FactorResult, AuthError>;
}

#[cfg(test)]
mod tests {
    use super::{FactorChallenge, FactorResponse, FactorType, MfaPurpose};
    use serde_json::json;

    #[test]
    fn factor_type_serializes_as_snake_case() {
        let value = serde_json::to_value(FactorType::WebAuthn).expect("factor type serializes");

        assert_eq!(value, json!("web_authn"));
    }

    #[test]
    fn challenge_and_response_are_json_serializable() {
        let challenge = FactorChallenge {
            challenge_id: "challenge-1".to_owned(),
            user_id: 7,
            factor_type: FactorType::Totp,
            purpose: MfaPurpose::TerminalStepUp,
            message: "Enter your one-time code".to_owned(),
        };
        let value = serde_json::to_value(&challenge).expect("challenge serializes");

        assert_eq!(value["factor_type"], "totp");
        assert_eq!(value["purpose"], "terminal_step_up");

        let response: FactorResponse = serde_json::from_value(json!({
            "challenge_id": "challenge-1",
            "factor_type": "totp",
            "secret": "123456"
        }))
        .expect("response deserializes");

        assert_eq!(response.factor_type, FactorType::Totp);
        assert_eq!(response.secret, "123456");
    }
}
