use axum::{extract::State, http::StatusCode, response::IntoResponse, Extension, Json};
use serde::Serialize;
use sunbolt_audit::AuditEvent;
use sunbolt_auth::Permission;

use crate::{auth::AuthenticatedUser, error::ErrorResponse, state::AppState};

#[derive(Debug, Serialize)]
struct AuditEntriesResponse {
    events: Vec<AuditEvent>,
}

pub(crate) async fn access_history(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> impl IntoResponse {
    crate::observability::record_actor_email(&user.0.email);
    if !state
        .auth
        .user_has_control_plane_permission(&user.0, Permission::AUDIT_VIEW)
    {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "audit log access is not permitted",
            }),
        )
            .into_response();
    }

    Json(AuditEntriesResponse {
        events: state.audit.access_history(),
    })
    .into_response()
}

pub(crate) async fn audit_logs(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> impl IntoResponse {
    crate::observability::record_actor_email(&user.0.email);
    if !state
        .auth
        .user_has_control_plane_permission(&user.0, Permission::AUDIT_VIEW)
    {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "audit log access is not permitted",
            }),
        )
            .into_response();
    }

    Json(AuditEntriesResponse {
        events: state.audit.events(),
    })
    .into_response()
}
