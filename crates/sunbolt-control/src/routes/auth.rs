use axum::{
    extract::State,
    http::{header::SET_COOKIE, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use sunbolt_audit::{AuditEventInput, AuditEventKind};
use sunbolt_auth::{AuthError, FactorType, User};

use crate::{
    auth::{authorize_terminal_request, session_token_from_headers, AuthenticatedUser},
    error::ErrorResponse,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub(crate) struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    user: User,
}

#[derive(Debug, Serialize)]
struct CurrentUserResponse {
    user: User,
}

#[derive(Debug, Serialize)]
struct TerminalAccessResponse {
    accepted: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StepUpMfaRequest {
    factor_type: FactorType,
}

#[derive(Debug, Serialize)]
struct StepUpMfaResponse {
    accepted: bool,
    factor_type: FactorType,
}

pub(crate) async fn auth_login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> impl IntoResponse {
    crate::observability::record_actor_email(&request.email);
    if !state.login_rate_limiter.check_and_record(&request.email) {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::UserLoginFailed,
            actor_email: Some(request.email.clone()),
            message: "login rate limit exceeded".to_owned(),
        });
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorResponse {
                error: "too many login attempts",
            }),
        )
            .into_response();
    }
    match state.auth.login(&request.email, &request.password) {
        Ok((user, session_token)) => {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::UserLoginSuccess,
                actor_email: Some(user.email.clone()),
                message: "user authenticated".to_owned(),
            });
            let mut response = Json(LoginResponse { user }).into_response();
            match HeaderValue::from_str(&state.auth.session_cookie_header(&session_token)) {
                Ok(cookie) => {
                    response.headers_mut().append(SET_COOKIE, cookie);
                    response
                }
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "failed to set auth cookie",
                    }),
                )
                    .into_response(),
            }
        }
        Err(AuthError::InvalidCredentials) => (StatusCode::UNAUTHORIZED, {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::UserLoginFailed,
                actor_email: Some(request.email),
                message: "login rejected".to_owned(),
            });
            Json(ErrorResponse {
                error: "invalid credentials",
            })
        })
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "auth service unavailable",
            }),
        )
            .into_response(),
    }
}

pub(crate) async fn auth_logout(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
) -> impl IntoResponse {
    crate::observability::record_actor_email(&user.0.email);
    if let Some(token) = session_token_from_headers(&headers) {
        let _ = state.auth.logout(token);
    }
    state.audit.record(AuditEventInput {
        kind: AuditEventKind::UserLogout,
        actor_email: Some(user.0.email.clone()),
        message: "user logged out".to_owned(),
    });

    let mut response = Json(CurrentUserResponse { user: user.0 }).into_response();
    if let Ok(cookie) = HeaderValue::from_str(&state.auth.clear_session_cookie_header()) {
        response.headers_mut().append(SET_COOKIE, cookie);
    }
    response
}

pub(crate) async fn auth_me(Extension(user): Extension<AuthenticatedUser>) -> impl IntoResponse {
    crate::observability::record_actor_email(&user.0.email);
    Json(CurrentUserResponse { user: user.0 })
}

pub(crate) async fn auth_terminal_access(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    match authorize_terminal_request(&state.auth, &headers) {
        Ok(user) => {
            crate::observability::record_actor_email(&user.email);
            Json(TerminalAccessResponse { accepted: true }).into_response()
        }
        Err(error) => (
            error.status_code(),
            Json(ErrorResponse {
                error: error.message(),
            }),
        )
            .into_response(),
    }
}

pub(crate) async fn auth_mfa_step_up(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Json(request): Json<StepUpMfaRequest>,
) -> impl IntoResponse {
    crate::observability::record_actor_email(&user.0.email);
    state.audit.record(AuditEventInput {
        kind: AuditEventKind::UserMfaChallenge,
        actor_email: Some(user.0.email.clone()),
        message: format!(
            "step-up MFA challenge requested using {:?}",
            request.factor_type
        ),
    });

    let Some(token) = session_token_from_headers(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "missing auth session",
            }),
        )
            .into_response();
    };
    match state.auth.record_mfa_success(token) {
        Ok(()) => {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::UserMfaSuccess,
                actor_email: Some(user.0.email),
                message: format!("step-up MFA completed using {:?}", request.factor_type),
            });
            Json(StepUpMfaResponse {
                accepted: true,
                factor_type: request.factor_type,
            })
            .into_response()
        }
        Err(AuthError::InvalidSession) => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid auth session",
            }),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "auth service unavailable",
            }),
        )
            .into_response(),
    }
}
