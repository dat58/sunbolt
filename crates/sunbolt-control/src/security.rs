use std::borrow::Cow;

use axum::http::{header, HeaderMap, Method};

/// Content-Security-Policy value for the Sunbolt web app.
///
/// - `default-src 'self'`: blocks all unspecified sources.
/// - `script-src 'wasm-unsafe-eval'`: required by Dioxus/WASM bundles.
/// - `connect-src ws: wss:`: allows WebSocket connections (terminal stream).
/// - `frame-ancestors 'none'`: prevents clickjacking.
pub const CSP_HEADER_VALUE: &str = "default-src 'self'; \
    script-src 'self' 'wasm-unsafe-eval'; \
    style-src 'self' 'unsafe-inline'; \
    connect-src 'self' ws: wss:; \
    img-src 'self' data:; \
    frame-ancestors 'none'";

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

/// Returns a copy of `text` with hex strings of 32 or more characters replaced
/// by `[REDACTED]`, masking auth tokens and similar opaque secrets.
///
/// When no redaction is needed the original string slice is returned without
/// any allocation.
#[must_use]
pub fn redact_sensitive(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut result = String::new();
    let mut last_end = 0;
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i].is_ascii_hexdigit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            if i - start >= 32 {
                if result.is_empty() {
                    result.reserve(text.len());
                }
                result.push_str(&text[last_end..start]);
                result.push_str("[REDACTED]");
                last_end = i;
            }
        } else {
            i += 1;
        }
    }

    if last_end == 0 {
        Cow::Borrowed(text)
    } else {
        result.push_str(&text[last_end..]);
        Cow::Owned(result)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        allows_any_origin, is_allowed_origin, is_cors_preflight, redact_sensitive, request_origin,
        CSP_HEADER_VALUE,
    };
    use axum::http::{header, header::ORIGIN, HeaderMap, HeaderValue, Method};

    fn headers_with_origin(origin: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
        headers
    }

    #[test]
    fn csp_header_contains_required_directives() {
        assert!(CSP_HEADER_VALUE.contains("default-src 'self'"));
        assert!(CSP_HEADER_VALUE.contains("frame-ancestors 'none'"));
        assert!(CSP_HEADER_VALUE.contains("connect-src 'self' ws: wss:"));
        assert!(CSP_HEADER_VALUE.contains("wasm-unsafe-eval"));
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
}
