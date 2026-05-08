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
    let node_id = request.node_id.clone();
    match state.node_enrollment.heartbeat(request) {
        Ok(node) => Json(AgentHeartbeatResponse {
            accepted: true,
            node,
        })
        .into_response(),
        Err(error @ NodeConnectionError::Revoked) => {
            record_agent_authentication_failed(&state, &node_id, error);
            (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "node revoked",
                }),
            )
                .into_response()
        }
        Err(
            error @ (NodeConnectionError::UnknownNode
            | NodeConnectionError::InvalidCredential
            | NodeConnectionError::CredentialExpired),
        ) => {
            record_agent_authentication_failed(&state, &node_id, error);
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "invalid node credential",
                }),
            )
                .into_response()
        }
    }
}

fn record_agent_authentication_failed(state: &AppState, node_id: &str, error: NodeConnectionError) {
    state.audit.record(AuditEventInput {
        kind: AuditEventKind::AgentAuthenticationFailed,
        actor_email: None,
        message: format!(
            "agent authentication failed for node {node_id}: {}",
            node_connection_error_reason(error)
        ),
    });
}

const fn node_connection_error_reason(error: NodeConnectionError) -> &'static str {
    match error {
        NodeConnectionError::UnknownNode => "unknown node",
        NodeConnectionError::InvalidCredential => "invalid credential",
        NodeConnectionError::CredentialExpired => "credential expired",
        NodeConnectionError::Revoked => "node revoked",
    }
}
