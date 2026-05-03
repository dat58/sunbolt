use std::{
    fmt::Write as _,
    time::{SystemTime, UNIX_EPOCH},
};

use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    AuthContext, AuthError, AuthFactor, AuthFactorEnrollment, FactorChallenge, FactorResponse,
    FactorResult, FactorType,
};

const DEFAULT_SECRET_LEN: usize = 20;
const DEFAULT_DIGITS: u32 = 6;
const DEFAULT_PERIOD_SECS: u64 = 30;
const DEFAULT_SKEW_STEPS: u64 = 1;
const HMAC_BLOCK_SIZE: usize = 64;

/// TOTP generator and verifier configuration.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TotpConfig {
    pub issuer: String,
    pub digits: u32,
    pub period_secs: u64,
    pub skew_steps: u64,
}

impl Default for TotpConfig {
    fn default() -> Self {
        Self {
            issuer: "Sunbolt".to_owned(),
            digits: DEFAULT_DIGITS,
            period_secs: DEFAULT_PERIOD_SECS,
            skew_steps: DEFAULT_SKEW_STEPS,
        }
    }
}

/// Secret material for a TOTP factor.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TotpSecret {
    bytes: Vec<u8>,
}

impl TotpSecret {
    /// Generates a new random TOTP secret.
    #[must_use]
    pub fn generate() -> Self {
        let mut bytes = vec![0_u8; DEFAULT_SECRET_LEN];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        Self { bytes }
    }

    /// Creates a TOTP secret from raw bytes.
    #[must_use]
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }

    /// Returns the secret encoded for authenticator apps.
    #[must_use]
    pub fn to_base32(&self) -> String {
        base32_encode(&self.bytes)
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// TOTP enrollment payload returned to the setup UI.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TotpEnrollment {
    pub factor: AuthFactorEnrollment,
    pub secret_base32: String,
    pub provisioning_uri: String,
    pub qr_code_payload: String,
}

/// Recovery guidance when a TOTP challenge cannot be satisfied.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TotpRecoveryPath {
    pub user_id: u64,
    pub fallback_factors: Vec<FactorType>,
    pub message: String,
}

/// Concrete TOTP MFA factor.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TotpFactor {
    secret: TotpSecret,
    config: TotpConfig,
}

impl TotpFactor {
    #[must_use]
    pub fn new(secret: TotpSecret, config: TotpConfig) -> Self {
        Self { secret, config }
    }

    /// Generates a code for the provided Unix timestamp.
    #[must_use]
    pub fn code_at(&self, unix_time_secs: u64) -> String {
        let counter = unix_time_secs / self.config.period_secs.max(1);
        hotp_sha256(self.secret.bytes(), counter, self.config.digits)
    }

    #[must_use]
    pub fn provisioning_uri(&self, account_name: &str) -> String {
        provisioning_uri(&self.config, account_name, &self.secret)
    }

    fn verify_code_at(&self, code: &str, unix_time_secs: u64) -> bool {
        let period = self.config.period_secs.max(1);
        let counter = unix_time_secs / period;
        let min_counter = counter.saturating_sub(self.config.skew_steps);
        let max_counter = counter.saturating_add(self.config.skew_steps);

        (min_counter..=max_counter).any(|candidate| {
            constant_time_eq(
                hotp_sha256(self.secret.bytes(), candidate, self.config.digits).as_bytes(),
                code.as_bytes(),
            )
        })
    }
}

impl AuthFactor for TotpFactor {
    fn factor_type(&self) -> FactorType {
        FactorType::Totp
    }

    fn begin_challenge(&self, ctx: &AuthContext) -> Result<FactorChallenge, AuthError> {
        Ok(FactorChallenge {
            challenge_id: format!("totp-{}-{}", ctx.user.id, now_unix_secs()),
            user_id: ctx.user.id,
            factor_type: FactorType::Totp,
            purpose: ctx.purpose,
            message: "Enter the code from your authenticator app".to_owned(),
        })
    }

    fn verify_challenge(
        &self,
        ctx: &AuthContext,
        response: FactorResponse,
    ) -> Result<FactorResult, AuthError> {
        Ok(FactorResult {
            user_id: ctx.user.id,
            factor_type: FactorType::Totp,
            verified: self.verify_code_at(response.secret.trim(), now_unix_secs()),
        })
    }
}

pub(crate) fn build_totp_enrollment(
    factor: AuthFactorEnrollment,
    user_email: &str,
    config: &TotpConfig,
    secret: &TotpSecret,
) -> TotpEnrollment {
    let provisioning_uri = provisioning_uri(config, user_email, secret);
    TotpEnrollment {
        factor,
        secret_base32: secret.to_base32(),
        provisioning_uri: provisioning_uri.clone(),
        qr_code_payload: provisioning_uri,
    }
}

pub(crate) fn default_totp_recovery_path(user_id: u64) -> TotpRecoveryPath {
    TotpRecoveryPath {
        user_id,
        fallback_factors: vec![FactorType::RecoveryCode, FactorType::AdminApproval],
        message: "Use a recovery code or contact an administrator to regain access".to_owned(),
    }
}

fn provisioning_uri(config: &TotpConfig, account_name: &str, secret: &TotpSecret) -> String {
    let issuer = url_encode(&config.issuer);
    let account_name = url_encode(account_name);
    format!(
        "otpauth://totp/{issuer}:{account_name}?secret={secret}&issuer={issuer}&algorithm=SHA256&digits={digits}&period={period}",
        secret = secret.to_base32(),
        digits = config.digits,
        period = config.period_secs.max(1),
    )
}

fn hotp_sha256(secret: &[u8], counter: u64, digits: u32) -> String {
    let hmac = hmac_sha256(secret, &counter.to_be_bytes());
    let offset = usize::from(hmac[hmac.len() - 1] & 0x0f);
    let binary = (u32::from(hmac[offset] & 0x7f) << 24)
        | (u32::from(hmac[offset + 1]) << 16)
        | (u32::from(hmac[offset + 2]) << 8)
        | u32::from(hmac[offset + 3]);
    let modulo = 10_u32.pow(digits);
    format!("{:0width$}", binary % modulo, width = digits as usize)
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut key_block = [0_u8; HMAC_BLOCK_SIZE];
    if key.len() > HMAC_BLOCK_SIZE {
        let digest = Sha256::digest(key);
        key_block[..digest.len()].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut outer_pad = [0x5c_u8; HMAC_BLOCK_SIZE];
    let mut inner_pad = [0x36_u8; HMAC_BLOCK_SIZE];
    for index in 0..HMAC_BLOCK_SIZE {
        outer_pad[index] ^= key_block[index];
        inner_pad[index] ^= key_block[index];
    }

    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_hash);
    outer.finalize().to_vec()
}

fn base32_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut output = String::new();
    let mut buffer = 0_u32;
    let mut bits_left = 0_u8;

    for byte in bytes {
        buffer = (buffer << 8) | u32::from(*byte);
        bits_left += 8;
        while bits_left >= 5 {
            let index = ((buffer >> (bits_left - 5)) & 0x1f) as usize;
            output.push(char::from(ALPHABET[index]));
            bits_left -= 5;
        }
    }

    if bits_left > 0 {
        let index = ((buffer << (5 - bits_left)) & 0x1f) as usize;
        output.push(char::from(ALPHABET[index]));
    }

    output
}

fn url_encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(char::from(byte));
            }
            _ => {
                let _ = write!(output, "%{byte:02X}");
            }
        }
    }
    output
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut diff = 0_u8;
    for (left, right) in left.iter().zip(right) {
        diff |= left ^ right;
    }
    diff == 0
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::{
        base32_encode, default_totp_recovery_path, hotp_sha256, TotpConfig, TotpFactor, TotpSecret,
    };
    use crate::{AuthContext, AuthFactor, FactorResponse, FactorType, MfaPurpose, User, UserRole};

    #[test]
    fn base32_encoding_omits_padding() {
        assert_eq!(base32_encode(b"foo"), "MZXW6");
    }

    #[test]
    fn totp_code_is_stable_for_fixed_secret_and_time() {
        let factor = TotpFactor::new(
            TotpSecret::from_bytes(b"12345678901234567890".to_vec()),
            TotpConfig::default(),
        );

        assert_eq!(
            factor.code_at(59),
            hotp_sha256(b"12345678901234567890", 1, 6)
        );
    }

    #[test]
    fn provisioning_uri_contains_qr_payload_details() {
        let factor = TotpFactor::new(
            TotpSecret::from_bytes(b"sunbolt-secret".to_vec()),
            TotpConfig::default(),
        );
        let uri = factor.provisioning_uri("admin@example.com");

        assert!(uri.starts_with("otpauth://totp/Sunbolt:admin%40example.com"));
        assert!(uri.contains("algorithm=SHA256"));
        assert!(uri.contains("digits=6"));
        assert!(uri.contains("period=30"));
    }

    #[test]
    fn totp_factor_rejects_wrong_code() {
        let factor = TotpFactor::new(
            TotpSecret::from_bytes(b"sunbolt-secret".to_vec()),
            TotpConfig::default(),
        );
        let ctx = AuthContext {
            user: User {
                id: 1,
                email: "admin@example.com".to_owned(),
                role: UserRole::Admin,
            },
            purpose: MfaPurpose::Login,
        };

        let result = factor
            .verify_challenge(
                &ctx,
                FactorResponse {
                    challenge_id: "challenge-1".to_owned(),
                    factor_type: FactorType::Totp,
                    secret: "000000".to_owned(),
                },
            )
            .expect("verification should return a result");

        assert!(!result.verified);
    }

    #[test]
    fn recovery_path_points_to_non_totp_fallbacks() {
        let recovery = default_totp_recovery_path(7);

        assert_eq!(recovery.user_id, 7);
        assert_eq!(
            recovery.fallback_factors,
            vec![FactorType::RecoveryCode, FactorType::AdminApproval]
        );
    }
}
