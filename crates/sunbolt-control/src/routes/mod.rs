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
    auth::require_auth_middleware,
    error::ErrorResponse,
    security,
    state::AppState,
    terminal::{spawn_session_cleanup_worker, terminal_websocket},
};

/// WebSocket path for browser terminal connections.
pub const TERMINAL_WS_PATH: &str = "/terminal/ws";
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
pub const NODES_PATH: &str = "/nodes";

/// Builds the control-plane router.
pub fn router() -> Router {
    build_router(AppState::from_env())
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
            "/nodes/{node_id}",
            get(node::node_details).layer(auth_layer.clone()),
        )
        .route(
            "/nodes/{node_id}/revoke",
            post(node::revoke_node).layer(auth_layer.clone()),
        )
        .route(
            ENROLLMENT_TOKENS_PATH,
            post(node::create_enrollment_token).layer(auth_layer),
        )
        .route(AGENT_ENROLL_PATH, post(agent::agent_enroll))
        .route(AGENT_HEARTBEAT_PATH, post(agent::agent_heartbeat))
        .layer(origin_layer)
        .layer(from_fn(security_headers_middleware))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok",
        component: "sunbolt-control",
    })
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    component: &'static str,
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
        HeaderValue::from_static("content-type"),
    );
    headers.append(header::VARY, HeaderValue::from_static("Origin"));
}
