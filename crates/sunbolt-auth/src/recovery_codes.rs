use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{AuthFactorEnrollment, FactorType};

const DEFAULT_CODE_COUNT: usize = 10;
const RECOVERY_CODE_BYTES: usize = 10;

/// Plaintext recovery codes returned once during enrollment or regeneration.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryCodeBatch {
    pub factor: AuthFactorEnrollment,
    pub codes: Vec<String>,
}

/// Stored recovery code hash metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct StoredRecoveryCode {
    pub hash: String,
    pub used: bool,
}

#[must_use]
pub(crate) fn generate_recovery_codes() -> Vec<String> {
    (0..DEFAULT_CODE_COUNT)
        .map(|_| generate_recovery_code())
        .collect()
}

#[must_use]
pub(crate) fn hash_recovery_code(code: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"sunbolt-recovery-code-v1:");
    hasher.update(normalize_recovery_code(code).as_bytes());
    format!("{:x}", hasher.finalize())
}

#[must_use]
pub(crate) fn recovery_code_records(codes: &[String]) -> Vec<StoredRecoveryCode> {
    codes
        .iter()
        .map(|code| StoredRecoveryCode {
            hash: hash_recovery_code(code),
            used: false,
        })
        .collect()
}

#[must_use]
pub(crate) fn build_recovery_code_batch(
    factor: AuthFactorEnrollment,
    codes: Vec<String>,
) -> RecoveryCodeBatch {
    RecoveryCodeBatch { factor, codes }
}

#[must_use]
pub(crate) fn recovery_code_factor_label() -> &'static str {
    "Recovery codes"
}

#[must_use]
pub(crate) fn recovery_code_factor_type() -> FactorType {
    FactorType::RecoveryCode
}

fn generate_recovery_code() -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0_u8; RECOVERY_CODE_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);

    let mut code = String::with_capacity(RECOVERY_CODE_BYTES * 2 + 2);
    for (index, byte) in bytes.into_iter().enumerate() {
        if index == 4 || index == 7 {
            code.push('-');
        }
        code.push(char::from(HEX[usize::from(byte >> 4)]));
        code.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    code
}

fn normalize_recovery_code(code: &str) -> String {
    code.chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{generate_recovery_codes, hash_recovery_code, recovery_code_records};

    #[test]
    fn generates_default_recovery_code_batch() {
        let codes = generate_recovery_codes();

        assert_eq!(codes.len(), 10);
        assert!(codes.iter().all(|code| code.len() == 22));
    }

    #[test]
    fn hashes_recovery_codes_after_normalization() {
        assert_eq!(
            hash_recovery_code("ABCD-1234"),
            hash_recovery_code(" abcd-1234 ")
        );
    }

    #[test]
    fn stores_hashes_without_plaintext_codes() {
        let codes = vec!["abcd-1234".to_owned()];
        let records = recovery_code_records(&codes);

        assert_eq!(records.len(), 1);
        assert_ne!(records[0].hash, codes[0]);
        assert!(!records[0].used);
    }
}
