use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    extract::{MatchedPath, Request},
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use tracing::{field, info_span, Span};

pub(crate) const REQUEST_ID_HEADER: &str = "x-request-id";

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct RequestId(String);

impl RequestId {
    #[must_use]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(valid_request_id)
        .unwrap_or_else(generate_request_id);
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));

    let mut response = next.run(request).await;
    if let Ok(header_value) = HeaderValue::from_str(&request_id) {
        response
            .headers_mut()
            .insert(REQUEST_ID_HEADER, header_value);
    }
    response
}

pub(crate) fn make_request_span(request: &Request) -> Span {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .map_or("missing", RequestId::as_str);
    let route_id = request.extensions().get::<MatchedPath>().map_or_else(
        || request.uri().path().to_owned(),
        |path| path.as_str().to_owned(),
    );

    info_span!(
        "http.request",
        request_id = %request_id,
        method = %request.method(),
        route_id = %route_id,
        path = %request.uri().path(),
        actor_id = field::Empty,
        actor_email = field::Empty,
        node_id = field::Empty,
        session_id = field::Empty,
        transport_id = field::Empty,
    )
}

pub(crate) fn record_actor_email(actor_email: &str) {
    Span::current().record("actor_email", actor_email);
}

pub(crate) fn record_node_id(node_id: &str) {
    Span::current().record("node_id", node_id);
}

pub(crate) fn record_session_id(session_id: &str) {
    Span::current().record("session_id", session_id);
}

pub(crate) fn record_transport_id(transport_id: &str) {
    Span::current().record("transport_id", transport_id);
}

pub(crate) fn record_route_id(route_id: &str) {
    Span::current().record("route_id", route_id);
}

fn generate_request_id() -> String {
    let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    format!("req-{id:016x}")
}

fn valid_request_id(value: &HeaderValue) -> Option<String> {
    let text = value.to_str().ok()?;
    if text.is_empty() || text.len() > 128 {
        return None;
    }
    if text
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        Some(text.to_owned())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::{generate_request_id, valid_request_id};

    #[test]
    fn generated_request_ids_are_stable_and_prefixed() {
        let request_id = generate_request_id();

        assert!(request_id.starts_with("req-"));
        assert_eq!(request_id.len(), 20);
    }

    #[test]
    fn request_id_validation_rejects_log_hostile_values() {
        assert_eq!(
            valid_request_id(&HeaderValue::from_static("client-request-1")).as_deref(),
            Some("client-request-1")
        );
        assert!(valid_request_id(&HeaderValue::from_static("bad request")).is_none());
        assert!(valid_request_id(&HeaderValue::from_static("")).is_none());
    }
}
