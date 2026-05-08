use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use sunbolt_audit::{AuditEventInput, AuditEventKind};

use crate::{
    agent::{AgentEnrollmentRequest, AgentHeartbeatRequest, AgentHeartbeatResponse},
    error::{EnrollmentError, ErrorResponse, NodeConnectionError},
    state::AppState,
};

pub(crate) async fn agent_enroll(
    State(state): State<AppState>,
    Json(request): Json<AgentEnrollmentRequest>,
) -> impl IntoResponse {
    match state.node_enrollment.enroll(request) {
        Ok(response) => {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::NodeEnrolled,
                actor_email: None,
                message: format!("node {} enrolled", response.node_id),
            });
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(
            EnrollmentError::InvalidToken
            | EnrollmentError::TokenUsed
            | EnrollmentError::TokenExpired,
        ) => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid enrollment token",
            }),
        )
            .into_response(),
    }
}

pub(crate) async fn agent_heartbeat(
    State(state): State<AppState>,
    Json(request): Json<AgentHeartbeatRequest>,
) -> impl IntoResponse {
    match state.node_enrollment.heartbeat(request) {
        Ok(node) => Json(AgentHeartbeatResponse {
            accepted: true,
            node,
        })
        .into_response(),
        Err(NodeConnectionError::UnknownNode | NodeConnectionError::InvalidCredential) => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid node credential",
            }),
        )
            .into_response(),
        Err(NodeConnectionError::Revoked) => (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "node revoked",
            }),
        )
            .into_response(),
    }
}
