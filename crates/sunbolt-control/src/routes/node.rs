use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use sunbolt_audit::{AuditEventInput, AuditEventKind};
use sunbolt_auth::Permission;
use sunbolt_protocol::{TerminalErrorCode, TerminalServerMessage};

use crate::{
    audit,
    auth::AuthenticatedUser,
    error::{ErrorResponse, NodeConnectionError},
    node::NodeView,
    security,
    state::AppState,
    terminal::{protocol_error_text, TerminalBackend},
};

#[derive(Debug, Deserialize)]
pub(crate) struct EnrollmentTokenRequest {
    expires_in_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
struct NodeListResponse {
    nodes: Vec<NodeView>,
}

#[derive(Debug, Serialize)]
struct NodeDetailsResponse {
    node: NodeView,
}

#[derive(Debug, Serialize)]
struct NodeCredentialRotationRouteResponse {
    rotation: crate::node::NodeCredentialRotationResponse,
}

pub(crate) async fn create_enrollment_token(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(request): Json<EnrollmentTokenRequest>,
) -> impl IntoResponse {
    crate::observability::record_actor_email(&user.0.email);
    let ttl = Duration::from_secs(request.expires_in_secs.unwrap_or(15 * 60).max(60));
    Json(state.node_enrollment.create_token(&user.0, ttl))
}

pub(crate) async fn list_nodes(State(state): State<AppState>) -> impl IntoResponse {
    Json(NodeListResponse {
        nodes: state.node_enrollment.list_nodes(),
    })
}

pub(crate) async fn node_details(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    crate::observability::record_node_id(&node_id);
    match state.node_enrollment.node_details(&node_id) {
        Some(node) => Json(NodeDetailsResponse { node }).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "node not found",
            }),
        )
            .into_response(),
    }
}

pub(crate) async fn rotate_node_credential(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    crate::observability::record_actor_email(&user.0.email);
    crate::observability::record_node_id(&node_id);
    match state
        .auth
        .user_has_node_permission(&user.0, &node_id, Permission::NODE_CREDENTIAL_ROTATE)
    {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "node credential rotation is not permitted",
                }),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "auth service unavailable",
                }),
            )
                .into_response();
        }
    }

    match state.node_enrollment.rotate_credential(&node_id) {
        Ok(rotation) => {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::NodeCredentialRotated,
                actor_email: Some(user.0.email),
                message: format!(
                    "node {node_id} credential rotated to {}",
                    rotation.credential.credential_fingerprint
                ),
            });
            Json(NodeCredentialRotationRouteResponse { rotation }).into_response()
        }
        Err(NodeConnectionError::UnknownNode) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "node not found",
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
        Err(NodeConnectionError::InvalidCredential | NodeConnectionError::CredentialExpired) => (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "node credential cannot be rotated",
            }),
        )
            .into_response(),
    }
}

pub(crate) async fn revoke_node(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    crate::observability::record_actor_email(&user.0.email);
    crate::observability::record_node_id(&node_id);
    match state.node_enrollment.revoke_node(&node_id) {
        Ok(node) => {
            let closed = state.sessions.close_sessions_for_node(&node_id);
            for (session_id, actor_email_session, output_tx, backend) in closed {
                let _ = output_tx.send(TerminalServerMessage::Error {
                    session_id: Some(session_id.clone()),
                    error: protocol_error_text(
                        TerminalErrorCode::TerminalUnavailable,
                        "node revoked",
                    ),
                });
                if let TerminalBackend::Remote { command_tx } = backend {
                    let _ = command_tx
                        .send(sunbolt_protocol::AgentTerminalCommand::CloseTerminal {
                            session_id: session_id.clone(),
                        })
                        .await;
                }
                audit::record_terminal_closed(
                    &state.audit,
                    actor_email_session,
                    security::redact_sensitive(&format!(
                        "terminal session {} closed: node {node_id} revoked",
                        session_id.0
                    ))
                    .into_owned(),
                );
            }
            state.agent_connections.disconnect(&node_id);
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::NodeRevoked,
                actor_email: Some(user.0.email),
                message: format!("node {node_id} revoked"),
            });
            Json(NodeDetailsResponse { node }).into_response()
        }
        Err(NodeConnectionError::UnknownNode) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "node not found",
            }),
        )
            .into_response(),
        Err(
            NodeConnectionError::InvalidCredential
            | NodeConnectionError::CredentialExpired
            | NodeConnectionError::Revoked,
        ) => (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "node is not allowed",
            }),
        )
            .into_response(),
    }
}
