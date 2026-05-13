pub(crate) mod agent;
pub(crate) mod audit;
pub(crate) mod auth;
pub(crate) mod node;

use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    middleware::{from_fn, from_fn_with_state, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use tower_http::trace::TraceLayer;

use crate::{
    agent::{agent_transport_long_poll, agent_transport_websocket},
    auth::require_auth_middleware,
    config::RuntimeMode,
    error::{ErrorResponse, StartupError},
    observability::{make_request_span, request_id_middleware, REQUEST_ID_HEADER},
    security,
    state::AppState,
    terminal::{
        list_active_terminal_sessions, list_detached_terminal_sessions,
        spawn_session_cleanup_worker, terminal_websocket, terminate_terminal_session,
    },
};

/// WebSocket path for browser terminal connections.
pub const TERMINAL_WS_PATH: &str = "/terminal/ws";
pub const TERMINAL_SESSIONS_ACTIVE_PATH: &str = "/terminal/sessions/active";
pub const TERMINAL_SESSIONS_DETACHED_PATH: &str = "/terminal/sessions/detached";
pub(crate) const TERMINAL_SESSION_TERMINATE_PATH: &str =
    "/terminal/sessions/{session_id}/terminate";
pub const HEALTH_PATH: &str = "/health";
pub const AUTH_LOGIN_PATH: &str = "/auth/login";
pub const AUTH_LOGOUT_PATH: &str = "/auth/logout";
pub const AUTH_ME_PATH: &str = "/auth/me";
pub const AUTH_TERMINAL_ACCESS_PATH: &str = "/auth/terminal-access";
pub const AUTH_MFA_STEP_UP_PATH: &str = "/auth/mfa/step-up";
pub const ACCESS_HISTORY_PATH: &str = "/access/history";
pub const AUDIT_LOGS_PATH: &str = "/audit/logs";
pub const ENROLLMENT_TOKENS_PATH: &str = "/nodes/enrollment-tokens";
pub const AGENT_ENROLL_PATH: &str = "/agent/enroll";
pub const AGENT_HEARTBEAT_PATH: &str = "/agent/heartbeat";
pub const AGENT_TRANSPORT_WS_PATH: &str = "/agent/transport/ws";
pub const AGENT_TRANSPORT_LONG_POLL_PATH: &str = "/agent/transport/long-poll";
pub const NODES_PATH: &str = "/nodes";
pub(crate) const NODE_DETAILS_PATH: &str = "/nodes/{node_id}";
pub(crate) const NODE_CREDENTIAL_ROTATE_PATH: &str = "/nodes/{node_id}/credentials/rotate";
pub(crate) const NODE_REVOKE_PATH: &str = "/nodes/{node_id}/revoke";

/// Builds the control-plane router for development mode.
///
/// # Panics
///
/// Panics when `SUNBOLT_ENV` is missing, set to `production`, or invalid.
/// Production startup must use [`try_router`] so durable storage can be
/// validated asynchronously before routes are served.
pub fn router() -> Router {
    match RuntimeMode::from_env() {
        Ok(RuntimeMode::Development) => build_router(AppState::development_from_env()),
        Ok(RuntimeMode::Production) => {
            panic!(
                "production router startup requires async `try_router` for durable storage validation"
            )
        }
        Err(error) => panic!("{error}"),
    }
}

/// Builds the control-plane router from environment configuration.
///
/// # Errors
///
/// Returns an error when production mode lacks required durable storage
/// configuration or the configured storage backend is unreachable.
pub async fn try_router() -> Result<Router, StartupError> {
    Ok(build_router(AppState::try_from_env().await?))
}

pub(crate) fn build_router(state: AppState) -> Router {
    spawn_session_cleanup_worker(
        state.sessions.clone(),
        state.terminal_config,
        state.audit.clone(),
    );
    let auth_layer = from_fn_with_state(state.auth.clone(), require_auth_middleware);
    let origin_layer = from_fn_with_state(state.allowed_origins.clone(), browser_origin_middleware);

    Router::new()
        .route(HEALTH_PATH, get(health))
        .route(TERMINAL_WS_PATH, get(terminal_websocket))
        .route(
            TERMINAL_SESSIONS_ACTIVE_PATH,
            get(list_active_terminal_sessions).layer(auth_layer.clone()),
        )
        .route(
            TERMINAL_SESSIONS_DETACHED_PATH,
            get(list_detached_terminal_sessions).layer(auth_layer.clone()),
        )
        .route(
            TERMINAL_SESSION_TERMINATE_PATH,
            post(terminate_terminal_session).layer(auth_layer.clone()),
        )
        .route(AUTH_LOGIN_PATH, post(auth::auth_login))
        .route(
            AUTH_MFA_STEP_UP_PATH,
            post(auth::auth_mfa_step_up).layer(auth_layer.clone()),
        )
        .route(
            AUTH_LOGOUT_PATH,
            post(auth::auth_logout).layer(auth_layer.clone()),
        )
        .route(AUTH_ME_PATH, get(auth::auth_me).layer(auth_layer.clone()))
        .route(
            AUTH_TERMINAL_ACCESS_PATH,
            get(auth::auth_terminal_access).layer(auth_layer.clone()),
        )
        .route(
            ACCESS_HISTORY_PATH,
            get(audit::access_history).layer(auth_layer.clone()),
        )
        .route(
            AUDIT_LOGS_PATH,
            get(audit::audit_logs).layer(auth_layer.clone()),
        )
        .route(NODES_PATH, get(node::list_nodes).layer(auth_layer.clone()))
        .route(
            NODE_DETAILS_PATH,
            get(node::node_details).layer(auth_layer.clone()),
        )
        .route(
            NODE_CREDENTIAL_ROTATE_PATH,
            post(node::rotate_node_credential).layer(auth_layer.clone()),
        )
        .route(
            NODE_REVOKE_PATH,
            post(node::revoke_node).layer(auth_layer.clone()),
        )
        .route(
            ENROLLMENT_TOKENS_PATH,
            post(node::create_enrollment_token).layer(auth_layer),
        )
        .route(AGENT_ENROLL_PATH, post(agent::agent_enroll))
        .route(AGENT_HEARTBEAT_PATH, post(agent::agent_heartbeat))
        .route(AGENT_TRANSPORT_WS_PATH, get(agent_transport_websocket))
        .route(
            AGENT_TRANSPORT_LONG_POLL_PATH,
            post(agent_transport_long_poll),
        )
        .layer(origin_layer)
        .layer(from_fn(security_headers_middleware))
        .layer(TraceLayer::new_for_http().make_span_with(make_request_span))
        .layer(from_fn(request_id_middleware))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let production_state = state.state_summary();
    Json(HealthResponse {
        status: "ok",
        component: "sunbolt-control",
        runtime_mode: production_state.runtime_mode,
        production_state,
    })
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    component: &'static str,
    runtime_mode: &'static str,
    production_state: crate::state::StateSummary,
}

pub(crate) async fn security_headers_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(security::CSP_HEADER_VALUE),
    );
    response
}

pub(crate) async fn browser_origin_middleware(
    State(allowed_origins): State<Vec<String>>,
    request: Request,
    next: Next,
) -> Response {
    if !security::is_allowed_origin(request.headers(), &allowed_origins) {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "cross-origin request rejected",
            }),
        )
            .into_response();
    }

    let request_origin = security::request_origin(request.headers()).map(str::to_owned);
    if security::is_cors_preflight(request.method(), request.headers()) {
        let mut response = StatusCode::NO_CONTENT.into_response();
        if let Some(origin) = request_origin.as_deref() {
            apply_cors_headers(response.headers_mut(), origin);
        }
        return response;
    }

    let mut response = next.run(request).await;
    if let Some(origin) = request_origin.as_deref() {
        apply_cors_headers(response.headers_mut(), origin);
    }
    response
}

fn apply_cors_headers(headers: &mut HeaderMap, origin: &str) {
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_str(origin).expect("request origin should be a valid header value"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, x-request-id"),
    );
    headers.insert(
        HeaderName::from_static("access-control-expose-headers"),
        HeaderValue::from_static(REQUEST_ID_HEADER),
    );
    headers.append(header::VARY, HeaderValue::from_static("Origin"));
}

#[cfg(test)]
mod tests {
    use super::{
        ACCESS_HISTORY_PATH, AGENT_ENROLL_PATH, AGENT_HEARTBEAT_PATH, AGENT_TRANSPORT_WS_PATH,
        AUDIT_LOGS_PATH, AUTH_LOGIN_PATH, AUTH_LOGOUT_PATH, AUTH_ME_PATH, AUTH_MFA_STEP_UP_PATH,
        AUTH_TERMINAL_ACCESS_PATH, ENROLLMENT_TOKENS_PATH, HEALTH_PATH, NODES_PATH,
        NODE_CREDENTIAL_ROTATE_PATH, NODE_DETAILS_PATH, NODE_REVOKE_PATH,
        TERMINAL_SESSIONS_ACTIVE_PATH, TERMINAL_SESSIONS_DETACHED_PATH,
        TERMINAL_SESSION_TERMINATE_PATH, TERMINAL_WS_PATH,
    };

    #[test]
    fn route_path_constants_preserve_public_api_paths() {
        assert_eq!(TERMINAL_WS_PATH, "/terminal/ws");
        assert_eq!(TERMINAL_SESSIONS_ACTIVE_PATH, "/terminal/sessions/active");
        assert_eq!(
            TERMINAL_SESSIONS_DETACHED_PATH,
            "/terminal/sessions/detached"
        );
        assert_eq!(
            TERMINAL_SESSION_TERMINATE_PATH,
            "/terminal/sessions/{session_id}/terminate"
        );
        assert_eq!(HEALTH_PATH, "/health");
        assert_eq!(AUTH_LOGIN_PATH, "/auth/login");
        assert_eq!(AUTH_LOGOUT_PATH, "/auth/logout");
        assert_eq!(AUTH_ME_PATH, "/auth/me");
        assert_eq!(AUTH_TERMINAL_ACCESS_PATH, "/auth/terminal-access");
        assert_eq!(AUTH_MFA_STEP_UP_PATH, "/auth/mfa/step-up");
        assert_eq!(ACCESS_HISTORY_PATH, "/access/history");
        assert_eq!(AUDIT_LOGS_PATH, "/audit/logs");
        assert_eq!(ENROLLMENT_TOKENS_PATH, "/nodes/enrollment-tokens");
        assert_eq!(AGENT_ENROLL_PATH, "/agent/enroll");
        assert_eq!(AGENT_HEARTBEAT_PATH, "/agent/heartbeat");
        assert_eq!(AGENT_TRANSPORT_WS_PATH, "/agent/transport/ws");
        assert_eq!(NODES_PATH, "/nodes");
        assert_eq!(NODE_DETAILS_PATH, "/nodes/{node_id}");
        assert_eq!(
            NODE_CREDENTIAL_ROTATE_PATH,
            "/nodes/{node_id}/credentials/rotate"
        );
        assert_eq!(NODE_REVOKE_PATH, "/nodes/{node_id}/revoke");
    }
}
