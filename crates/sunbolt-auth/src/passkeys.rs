use rand::RngCore;
use serde::{Deserialize, Serialize};

const CHALLENGE_BYTES: usize = 32;

/// `WebAuthn` crate selected for the full browser assertion implementation.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WebAuthnCrateChoice {
    pub crate_name: &'static str,
    pub rationale: &'static str,
}

/// Public key credential metadata stored after passkey registration.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PasskeyCredential {
    pub id: u64,
    pub user_id: u64,
    pub credential_id: String,
    pub public_key: String,
    pub label: String,
    pub enabled: bool,
}

/// Registration challenge sent to the browser before creating a passkey.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PasskeyRegistrationChallenge {
    pub challenge_id: String,
    pub user_id: u64,
    pub user_email: String,
    pub relying_party_id: String,
    pub relying_party_name: String,
    pub origin: String,
    pub challenge: String,
}

/// Authentication challenge sent to the browser before using a passkey.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PasskeyAuthenticationChallenge {
    pub challenge_id: String,
    pub user_id: u64,
    pub relying_party_id: String,
    pub origin: String,
    pub challenge: String,
    pub allowed_credential_ids: Vec<String>,
}

#[must_use]
pub fn recommended_webauthn_crate() -> WebAuthnCrateChoice {
    WebAuthnCrateChoice {
        crate_name: "webauthn-rs",
        rationale: "Use webauthn-rs for production WebAuthn ceremony validation; Sunbolt keeps only challenge and credential metadata in this MVP scaffold.",
    }
}

#[must_use]
pub(crate) fn registration_challenge(
    user_id: u64,
    user_email: &str,
    relying_party_id: &str,
    relying_party_name: &str,
    origin: &str,
) -> PasskeyRegistrationChallenge {
    let challenge = random_challenge();
    PasskeyRegistrationChallenge {
        challenge_id: format!("passkey-registration-{user_id}-{challenge}"),
        user_id,
        user_email: user_email.to_owned(),
        relying_party_id: relying_party_id.to_owned(),
        relying_party_name: relying_party_name.to_owned(),
        origin: origin.to_owned(),
        challenge,
    }
}

#[must_use]
pub(crate) fn authentication_challenge(
    user_id: u64,
    relying_party_id: &str,
    origin: &str,
    allowed_credential_ids: Vec<String>,
) -> PasskeyAuthenticationChallenge {
    let challenge = random_challenge();
    PasskeyAuthenticationChallenge {
        challenge_id: format!("passkey-authentication-{user_id}-{challenge}"),
        user_id,
        relying_party_id: relying_party_id.to_owned(),
        origin: origin.to_owned(),
        challenge,
        allowed_credential_ids,
    }
}

#[must_use]
pub(crate) fn credential(
    id: u64,
    user_id: u64,
    credential_id: &str,
    public_key: &str,
    label: &str,
) -> PasskeyCredential {
    PasskeyCredential {
        id,
        user_id,
        credential_id: credential_id.to_owned(),
        public_key: public_key.to_owned(),
        label: label.trim().to_owned(),
        enabled: true,
    }
}

fn random_challenge() -> String {
    let mut bytes = [0_u8; CHALLENGE_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    base64url_encode(&bytes)
}

fn base64url_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::new();
    let mut index = 0;

    while index + 3 <= bytes.len() {
        let chunk = u32::from(bytes[index]) << 16
            | u32::from(bytes[index + 1]) << 8
            | u32::from(bytes[index + 2]);
        output.push(char::from(ALPHABET[((chunk >> 18) & 0x3f) as usize]));
        output.push(char::from(ALPHABET[((chunk >> 12) & 0x3f) as usize]));
        output.push(char::from(ALPHABET[((chunk >> 6) & 0x3f) as usize]));
        output.push(char::from(ALPHABET[(chunk & 0x3f) as usize]));
        index += 3;
    }

    match bytes.len() - index {
        1 => {
            let chunk = u32::from(bytes[index]) << 16;
            output.push(char::from(ALPHABET[((chunk >> 18) & 0x3f) as usize]));
            output.push(char::from(ALPHABET[((chunk >> 12) & 0x3f) as usize]));
        }
        2 => {
            let chunk = u32::from(bytes[index]) << 16 | u32::from(bytes[index + 1]) << 8;
            output.push(char::from(ALPHABET[((chunk >> 18) & 0x3f) as usize]));
            output.push(char::from(ALPHABET[((chunk >> 12) & 0x3f) as usize]));
            output.push(char::from(ALPHABET[((chunk >> 6) & 0x3f) as usize]));
        }
        _ => {}
    }

    output
}

#[cfg(test)]
mod tests {
    use super::{
        authentication_challenge, base64url_encode, recommended_webauthn_crate,
        registration_challenge,
    };

    #[test]
    fn records_webauthn_crate_choice() {
        let choice = recommended_webauthn_crate();

        assert_eq!(choice.crate_name, "webauthn-rs");
        assert!(choice.rationale.contains("WebAuthn"));
    }

    #[test]
    fn base64url_encoding_omits_padding() {
        assert_eq!(base64url_encode(b"sunbolt"), "c3VuYm9sdA");
    }

    #[test]
    fn builds_registration_and_authentication_challenges() {
        let registration = registration_challenge(
            7,
            "admin@example.com",
            "localhost",
            "Sunbolt",
            "http://localhost:3000",
        );
        assert_eq!(registration.user_id, 7);
        assert_eq!(registration.relying_party_name, "Sunbolt");
        assert!(!registration.challenge.is_empty());

        let authentication = authentication_challenge(
            7,
            "localhost",
            "http://localhost:3000",
            vec!["credential-1".to_owned()],
        );
        assert_eq!(authentication.allowed_credential_ids, vec!["credential-1"]);
        assert!(!authentication.challenge.is_empty());
    }
}
