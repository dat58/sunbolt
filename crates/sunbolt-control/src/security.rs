use std::{
    borrow::Cow,
    hash::{Hash, Hasher},
};

use axum::http::{header, HeaderMap, Method};
use rand::RngCore;

pub(crate) const CSRF_HEADER_NAME: &str = "x-sunbolt-csrf";
pub(crate) const CSRF_HEADER_VALUE: &str = "1";

/// Content-Security-Policy value for the Sunbolt web app.
///
/// - `default-src 'self'`: blocks all unspecified sources.
/// - `script-src 'wasm-unsafe-eval'`: required by Dioxus/WASM bundles.
/// - `connect-src`: is generated from configured browser origins.
/// - `frame-ancestors 'none'`: prevents clickjacking.
const CSP_BASE_HEADER_VALUE: &str = "default-src 'self'; \
    script-src 'self' 'wasm-unsafe-eval'; \
    style-src 'self' 'unsafe-inline'; \
    img-src 'self' data:; \
    base-uri 'self'; \
    form-action 'self'; \
    object-src 'none'; \
    frame-ancestors 'none'";

/// Builds a Content-Security-Policy value for the web app.
///
/// Development may use an empty or wildcard origin list while local topology is
/// changing. Explicit production origins narrow `connect-src` to same-origin
/// HTTP plus the matching WebSocket origins needed by terminal sessions.
#[must_use]
pub fn content_security_policy(allowed_origins: &[String]) -> String {
    let mut connect_sources = vec!["'self'".to_owned()];

    if allows_any_origin(allowed_origins) {
        connect_sources.extend(["ws:".to_owned(), "wss:".to_owned()]);
    } else {
        for origin in allowed_origins {
            push_unique_source(&mut connect_sources, origin);
            if let Some(websocket_origin) = websocket_origin_for(origin) {
                push_unique_source(&mut connect_sources, &websocket_origin);
            }
        }
    }

    format!(
        "{CSP_BASE_HEADER_VALUE}; connect-src {}",
        connect_sources.join(" ")
    )
}

/// Returns `true` when the request origin is acceptable.
///
/// Rules:
/// - Empty `allowed_origins` ⇒ permissive dev mode; everything passes.
/// - No `Origin` header ⇒ same-origin or non-browser client; always allowed.
/// - Otherwise the `Origin` header value must appear in `allowed_origins`.
#[must_use]
pub fn is_allowed_origin(headers: &HeaderMap, allowed_origins: &[String]) -> bool {
    if allows_any_origin(allowed_origins) {
        return true;
    }
    let Some(origin) = request_origin(headers) else {
        return true;
    };
    allowed_origins.iter().any(|o| o == origin)
}

/// Returns `true` when the configured origin list is permissive.
#[must_use]
pub fn allows_any_origin(allowed_origins: &[String]) -> bool {
    allowed_origins.is_empty() || allowed_origins.iter().any(|origin| origin == "*")
}

/// Returns the request `Origin` header as UTF-8 when present.
#[must_use]
pub fn request_origin(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
}

/// Returns `true` when the request is a browser CORS preflight probe.
#[must_use]
pub fn is_cors_preflight(method: &Method, headers: &HeaderMap) -> bool {
    method == Method::OPTIONS
        && request_origin(headers).is_some()
        && headers.contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
}

/// Returns true when a browser state-changing request carries Sunbolt's CSRF
/// sentinel header. Cross-site forms cannot set this header, and CORS only
/// allows it for configured origins.
#[must_use]
pub fn has_valid_csrf_header(headers: &HeaderMap) -> bool {
    headers
        .get(CSRF_HEADER_NAME)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == CSRF_HEADER_VALUE)
}

/// Returns a copy of `text` with known secret material replaced by
/// `[REDACTED]`, masking auth tokens, cookies, node credentials, recovery
/// codes, passkey material, and similar opaque secrets.
///
/// When no redaction is needed the original string slice is returned without
/// any allocation.
#[must_use]
pub fn redact_sensitive(text: &str) -> Cow<'_, str> {
    sunbolt_audit::redact_sensitive(text)
}

pub(crate) fn random_token() -> String {
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

pub(crate) fn token_hash(token: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut hasher);
    hasher.finish()
}

fn push_unique_source(sources: &mut Vec<String>, source: &str) {
    if !sources.iter().any(|existing| existing == source) {
        sources.push(source.to_owned());
    }
}

fn websocket_origin_for(origin: &str) -> Option<String> {
    if let Some(rest) = origin.strip_prefix("https://") {
        Some(format!("wss://{rest}"))
    } else {
        origin
            .strip_prefix("http://")
            .map(|rest| format!("ws://{rest}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        allows_any_origin, content_security_policy, has_valid_csrf_header, is_allowed_origin,
        is_cors_preflight, redact_sensitive, request_origin, CSRF_HEADER_NAME,
    };
    use axum::http::{header, header::ORIGIN, HeaderMap, HeaderValue, Method};

    fn headers_with_origin(origin: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
        headers
    }

    #[test]
    fn csp_header_contains_required_directives() {
        let csp = content_security_policy(&[]);

        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(csp.contains("connect-src 'self' ws: wss:"));
        assert!(csp.contains("wasm-unsafe-eval"));
        assert!(csp.contains("base-uri 'self'"));
        assert!(csp.contains("form-action 'self'"));
        assert!(csp.contains("object-src 'none'"));
    }

    #[test]
    fn csp_connect_src_is_limited_to_explicit_origins() {
        let csp = content_security_policy(&[
            "https://control.example.com".to_owned(),
            "http://localhost:3000".to_owned(),
        ]);

        assert!(csp.contains("connect-src 'self'"));
        assert!(csp.contains("https://control.example.com"));
        assert!(csp.contains("wss://control.example.com"));
        assert!(csp.contains("http://localhost:3000"));
        assert!(csp.contains("ws://localhost:3000"));
        assert!(!connect_src_sources(&csp).contains(&"ws:"));
        assert!(!connect_src_sources(&csp).contains(&"wss:"));
    }

    #[test]
    fn csp_keeps_permissive_connect_src_for_development() {
        let csp = content_security_policy(&[]);

        assert!(csp.contains("connect-src 'self' ws: wss:"));
    }

    fn connect_src_sources(csp: &str) -> Vec<&str> {
        csp.split("; ")
            .find_map(|directive| directive.strip_prefix("connect-src "))
            .expect("connect-src should be present")
            .split_whitespace()
            .collect()
    }

    #[test]
    fn origin_allowed_when_list_is_empty() {
        let headers = headers_with_origin("http://evil.example.com");
        assert!(is_allowed_origin(&headers, &[]));
    }

    #[test]
    fn wildcard_origin_configuration_is_permissive() {
        let headers = headers_with_origin("http://evil.example.com");
        assert!(allows_any_origin(&["*".to_owned()]));
        assert!(is_allowed_origin(&headers, &["*".to_owned()]));
    }

    #[test]
    fn origin_allowed_when_present_in_list() {
        let allowed = vec!["http://localhost:3000".to_owned()];
        let headers = headers_with_origin("http://localhost:3000");
        assert!(is_allowed_origin(&headers, &allowed));
    }

    #[test]
    fn origin_rejected_when_not_in_list() {
        let allowed = vec!["http://localhost:3000".to_owned()];
        let headers = headers_with_origin("http://evil.example.com");
        assert!(!is_allowed_origin(&headers, &allowed));
    }

    #[test]
    fn no_origin_header_is_always_allowed() {
        let allowed = vec!["http://localhost:3000".to_owned()];
        assert!(is_allowed_origin(&HeaderMap::new(), &allowed));
    }

    #[test]
    fn request_origin_extracts_origin_header() {
        let headers = headers_with_origin("http://localhost:8080");
        assert_eq!(request_origin(&headers), Some("http://localhost:8080"));
    }

    #[test]
    fn cors_preflight_is_detected() {
        let mut headers = headers_with_origin("http://localhost:8080");
        headers.insert(
            header::ACCESS_CONTROL_REQUEST_METHOD,
            HeaderValue::from_static("POST"),
        );

        assert!(is_cors_preflight(&Method::OPTIONS, &headers));
        assert!(!is_cors_preflight(&Method::POST, &headers));
    }

    #[test]
    fn csrf_header_requires_exact_sentinel_value() {
        let mut headers = HeaderMap::new();
        assert!(!has_valid_csrf_header(&headers));

        headers.insert(CSRF_HEADER_NAME, HeaderValue::from_static("0"));
        assert!(!has_valid_csrf_header(&headers));

        headers.insert(CSRF_HEADER_NAME, HeaderValue::from_static("1"));
        assert!(has_valid_csrf_header(&headers));
    }

    #[test]
    fn redact_sensitive_masks_long_hex_strings() {
        let token = "a".repeat(64);
        let text = format!("session token={token} logged in");
        let redacted = redact_sensitive(&text);
        assert!(!redacted.contains(&token));
        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.contains("session token="));
        assert!(redacted.contains("logged in"));
    }

    #[test]
    fn redact_sensitive_keeps_short_hex_strings() {
        assert_eq!(redact_sensitive("id=deadbeef"), "id=deadbeef");
    }

    #[test]
    fn redact_sensitive_returns_borrowed_when_unchanged() {
        let text = "no secrets here";
        assert!(matches!(
            redact_sensitive(text),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn redact_sensitive_handles_multiple_tokens() {
        let t1 = "b".repeat(64);
        let t2 = "c".repeat(64);
        let text = format!("first={t1} second={t2}");
        let redacted = redact_sensitive(&text);
        assert_eq!(redacted, "first=[REDACTED] second=[REDACTED]");
    }

    #[test]
    fn redact_sensitive_masks_cookie_headers() {
        let text = "Cookie: sunbolt_session=session-1; theme=dark";
        let redacted = redact_sensitive(text);

        assert_eq!(redacted, "Cookie: [REDACTED]");
        assert!(!redacted.contains("session-1"));
    }
}
