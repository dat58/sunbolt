use std::borrow::Cow;

const REDACTION: &str = "[REDACTED]";

const SENSITIVE_KEYS: &[&str] = &[
    "authorization",
    "cookie",
    "set_cookie",
    "password",
    "secret",
    "token",
    "session_token",
    "reconnect_token",
    "enrollment_token",
    "sunbolt_agent_enrollment_token",
    "credential_secret",
    "credential_proof",
    "node_credential",
    "node_credentials",
    "recovery_code",
    "recovery_codes",
    "passkey_credential",
    "credential_id",
    "allowed_credential_ids",
    "public_key",
    "challenge",
    "challenge_id",
    "raw_id",
    "attestation_object",
    "client_data_json",
    "authenticator_data",
    "signature",
    "user_handle",
];

/// Returns a copy of `text` with known secret material replaced by
/// `[REDACTED]`.
///
/// The redactor covers structured key/value fields, HTTP cookie and
/// authorization headers, long opaque hex tokens, and plaintext recovery-code
/// shapes. When no redaction is needed the original string slice is returned
/// without allocation.
#[must_use]
pub fn redact_sensitive(text: &str) -> Cow<'_, str> {
    let with_headers = redact_sensitive_headers(text);
    let with_keys = redact_sensitive_key_values(with_headers.as_ref());
    let with_tokens = redact_sensitive_tokens(with_keys.as_ref());

    if with_tokens == text {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(with_tokens.into_owned())
    }
}

fn redact_sensitive_headers(text: &str) -> Cow<'_, str> {
    let mut changed = false;
    let mut output = String::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        let Some(line_end) = remaining.find('\n') else {
            append_redacted_header_line(&mut output, remaining, &mut changed);
            break;
        };
        let (line, rest) = remaining.split_at(line_end + 1);
        append_redacted_header_line(&mut output, line, &mut changed);
        remaining = rest;
    }

    if changed {
        Cow::Owned(output)
    } else {
        Cow::Borrowed(text)
    }
}

fn append_redacted_header_line(output: &mut String, line: &str, changed: &mut bool) {
    let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
    let newline = if line.ends_with('\n') { "\n" } else { "" };
    let trimmed_start = line_without_newline.trim_start();
    let prefix_len = line_without_newline.len() - trimmed_start.len();

    let Some((name, _value)) = trimmed_start.split_once(':') else {
        output.push_str(line);
        return;
    };

    if is_sensitive_header_name(name) {
        output.push_str(&line_without_newline[..prefix_len]);
        output.push_str(name);
        output.push_str(": ");
        output.push_str(REDACTION);
        output.push_str(newline);
        *changed = true;
    } else {
        output.push_str(line);
    }
}

fn is_sensitive_header_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("cookie")
        || name.eq_ignore_ascii_case("set-cookie")
        || name.eq_ignore_ascii_case("authorization")
}

fn redact_sensitive_key_values(text: &str) -> Cow<'_, str> {
    let mut output = String::new();
    let mut last_end = 0;
    let mut index = 0;

    while index < text.len() {
        let Some((_key_start, key_end, key)) = next_identifier(text, index) else {
            break;
        };
        index = key_end;
        if !is_sensitive_key(&key) {
            continue;
        }

        let Some((delimiter_start, delimiter_end)) = next_key_value_delimiter(text, key_end) else {
            continue;
        };
        let Some((value_start, value_end)) = value_bounds(text, delimiter_end) else {
            continue;
        };

        if output.is_empty() {
            output.reserve(text.len());
        }
        output.push_str(&text[last_end..delimiter_start]);
        output.push_str(&text[delimiter_start..value_start]);
        output.push_str(REDACTION);
        last_end = value_end;
        index = value_end;
    }

    if last_end == 0 {
        Cow::Borrowed(text)
    } else {
        output.push_str(&text[last_end..]);
        Cow::Owned(output)
    }
}

fn next_identifier(text: &str, start_at: usize) -> Option<(usize, usize, String)> {
    let bytes = text.as_bytes();
    let mut index = start_at;
    while index < bytes.len() {
        if is_identifier_byte(bytes[index])
            && (index == 0 || !is_identifier_byte(bytes[index.saturating_sub(1)]))
        {
            let start = index;
            while index < bytes.len() && is_identifier_byte(bytes[index]) {
                index += 1;
            }
            let normalized = normalize_key(&text[start..index]);
            return Some((start, index, normalized));
        }
        index += 1;
    }
    None
}

const fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn normalize_key(key: &str) -> String {
    key.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' => char::from(byte + 32),
            b'-' => '_',
            _ => char::from(byte),
        })
        .collect()
}

fn is_sensitive_key(key: &str) -> bool {
    SENSITIVE_KEYS
        .iter()
        .any(|sensitive| key == *sensitive || key.ends_with(&format!("_{sensitive}")))
}

fn next_key_value_delimiter(text: &str, start_at: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut index = start_at;

    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }

    if index < bytes.len() && bytes[index] == b'"' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
    }

    if index < bytes.len() && matches!(bytes[index], b':' | b'=') {
        Some((index, index + 1))
    } else {
        None
    }
}

fn value_bounds(text: &str, start_at: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut index = start_at;

    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }

    if index >= bytes.len() {
        return None;
    }

    match bytes[index] {
        b'"' | b'\'' => Some(quoted_value_bounds(text, index)),
        b'[' => Some((index, balanced_value_end(text, index, b'[', b']'))),
        b'{' => Some((index, balanced_value_end(text, index, b'{', b'}'))),
        _ => Some((index, unquoted_value_end(text, index))),
    }
}

fn quoted_value_bounds(text: &str, quote_start: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let quote = bytes[quote_start];
    let mut index = quote_start + 1;

    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = index.saturating_add(2);
            continue;
        }
        if bytes[index] == quote {
            return (quote_start + 1, index);
        }
        index += 1;
    }

    (quote_start + 1, text.len())
}

fn balanced_value_end(text: &str, start_at: usize, open: u8, close: u8) -> usize {
    let bytes = text.as_bytes();
    let mut index = start_at;
    let mut depth = 0_u32;

    while index < bytes.len() {
        match bytes[index] {
            byte if byte == open => depth += 1,
            byte if byte == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return index + 1;
                }
            }
            b'"' | b'\'' => {
                let (_start, end) = quoted_value_bounds(text, index);
                index = end;
            }
            _ => {}
        }
        index += 1;
    }

    text.len()
}

fn unquoted_value_end(text: &str, start_at: usize) -> usize {
    let bytes = text.as_bytes();
    let mut index = start_at;

    while index < bytes.len() && !matches!(bytes[index], b',' | b';' | b'\n' | b'\r') {
        if bytes[index].is_ascii_whitespace() {
            break;
        }
        index += 1;
    }

    index
}

fn redact_sensitive_tokens(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut output = String::new();
    let mut last_end = 0;
    let mut index = 0;

    while index < bytes.len() {
        if is_token_byte(bytes[index]) {
            let start = index;
            while index < bytes.len() && is_token_byte(bytes[index]) {
                index += 1;
            }
            let candidate = &text[start..index];
            if is_sensitive_token(candidate) {
                if output.is_empty() {
                    output.reserve(text.len());
                }
                output.push_str(&text[last_end..start]);
                output.push_str(REDACTION);
                last_end = index;
            }
        } else {
            index += 1;
        }
    }

    if last_end == 0 {
        Cow::Borrowed(text)
    } else {
        output.push_str(&text[last_end..]);
        Cow::Owned(output)
    }
}

const fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
}

fn is_sensitive_token(candidate: &str) -> bool {
    is_long_hex_token(candidate) || is_recovery_code(candidate)
}

fn is_long_hex_token(candidate: &str) -> bool {
    candidate.len() >= 32 && candidate.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_recovery_code(candidate: &str) -> bool {
    let bytes = candidate.as_bytes();
    bytes.len() == 22
        && bytes[8] == b'-'
        && bytes[15] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 8 | 15) || byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::redact_sensitive;

    #[test]
    fn redacts_cookie_headers() {
        let text = "Cookie: sunbolt_session=session-1; theme=dark";
        let redacted = redact_sensitive(text);

        assert_eq!(redacted, "Cookie: [REDACTED]");
        assert!(!redacted.contains("session-1"));
    }

    #[test]
    fn redacts_set_cookie_headers() {
        let text = "Set-Cookie: sunbolt_session=session-1; Path=/; HttpOnly";
        let redacted = redact_sensitive(text);

        assert_eq!(redacted, "Set-Cookie: [REDACTED]");
        assert!(!redacted.contains("session-1"));
    }

    #[test]
    fn redacts_enrollment_tokens() {
        let text = "SUNBOLT_AGENT_ENROLLMENT_TOKEN=enroll-secret cargo run";
        let redacted = redact_sensitive(text);

        assert_eq!(
            redacted,
            "SUNBOLT_AGENT_ENROLLMENT_TOKEN=[REDACTED] cargo run"
        );
        assert!(!redacted.contains("enroll-secret"));
    }

    #[test]
    fn redacts_node_credential_material() {
        let text = r#"{"credential_secret":"node-secret","credential_proof":"node-proof","node_id":"node-1"}"#;
        let redacted = redact_sensitive(text);

        assert!(redacted.contains(r#""credential_secret":"[REDACTED]""#));
        assert!(redacted.contains(r#""credential_proof":"[REDACTED]""#));
        assert!(redacted.contains(r#""node_id":"node-1""#));
        assert!(!redacted.contains("node-secret"));
        assert!(!redacted.contains("node-proof"));
    }

    #[test]
    fn redacts_recovery_codes_by_key_and_shape() {
        let code = "01234567-89abcd-ef0123";
        let text = format!(r#"recovery_codes=["{code}"] fallback={code}"#);
        let redacted = redact_sensitive(&text);

        assert_eq!(redacted, r"recovery_codes=[REDACTED] fallback=[REDACTED]");
        assert!(!redacted.contains(code));
    }

    #[test]
    fn redacts_passkey_credential_material() {
        let text = r#"credential_id="passkey-id" public_key="public-key" attestation_object="attestation""#;
        let redacted = redact_sensitive(text);

        assert_eq!(
            redacted,
            r#"credential_id="[REDACTED]" public_key="[REDACTED]" attestation_object="[REDACTED]""#
        );
    }

    #[test]
    fn redacts_authorization_headers() {
        let text = "Authorization: Bearer bearer-secret";
        let redacted = redact_sensitive(text);

        assert_eq!(redacted, "Authorization: [REDACTED]");
        assert!(!redacted.contains("bearer-secret"));
    }

    #[test]
    fn redacts_long_hex_strings() {
        let token = "a".repeat(64);
        let text = format!("session token={token} logged in");
        let redacted = redact_sensitive(&text);

        assert!(!redacted.contains(&token));
        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.contains("session token="));
        assert!(redacted.contains("logged in"));
    }

    #[test]
    fn keeps_short_hex_strings() {
        assert_eq!(redact_sensitive("id=deadbeef"), "id=deadbeef");
    }

    #[test]
    fn returns_borrowed_when_unchanged() {
        let text = "no secrets here";
        assert!(matches!(redact_sensitive(text), Cow::Borrowed(_)));
    }

    #[test]
    fn handles_multiple_tokens() {
        let first = "b".repeat(64);
        let second = "c".repeat(64);
        let text = format!("first={first} second={second}");
        let redacted = redact_sensitive(&text);

        assert_eq!(redacted, "first=[REDACTED] second=[REDACTED]");
    }
}
