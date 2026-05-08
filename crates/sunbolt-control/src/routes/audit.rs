use axum::{extract::State, response::IntoResponse, Json};
use serde::Serialize;
use sunbolt_audit::AuditEvent;

use crate::state::AppState;

#[derive(Debug, Serialize)]
struct AuditEntriesResponse {
    events: Vec<AuditEvent>,
}

pub(crate) async fn access_history(State(state): State<AppState>) -> impl IntoResponse {
    Json(AuditEntriesResponse {
        events: state.audit.access_history(),
    })
}

pub(crate) async fn audit_logs(State(state): State<AppState>) -> impl IntoResponse {
    Json(AuditEntriesResponse {
        events: state.audit.events(),
    })
}
