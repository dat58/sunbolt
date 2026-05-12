use axum::{
    extract::{Request, State},
    http::{header::COOKIE, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use sunbolt_auth::{AuthService, User, SESSION_COOKIE_NAME};

use crate::error::{ErrorResponse, TerminalAuthorizationError};

#[derive(Debug, Clone)]
pub(crate) struct AuthenticatedUser(pub(crate) User);

pub(crate) fn session_token_from_headers(headers: &HeaderMap) -> Option<&str> {
    let cookie_header = headers.get(COOKIE)?.to_str().ok()?;

    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        let (name, value) = cookie.split_once('=')?;
        if name == SESSION_COOKIE_NAME {
            return Some(value);
        }
    }

    None
}

pub(crate) fn authorize_terminal_request(
    auth: &AuthService,
    headers: &HeaderMap,
) -> Result<User, TerminalAuthorizationError> {
    let token =
        session_token_from_headers(headers).ok_or(TerminalAuthorizationError::Unauthorized)?;
    let user = match auth.current_user(token) {
        Ok(Some(user)) => user,
        Ok(None) => return Err(TerminalAuthorizationError::Unauthorized),
        Err(_) => return Err(TerminalAuthorizationError::Internal),
    };

    if !auth.can_open_terminal(&user) {
        return Err(TerminalAuthorizationError::Forbidden);
    }
    match auth.can_open_terminal_with_session(&user, token) {
        Ok(true) => {}
        Ok(false) if auth.terminal_step_up_policy_enabled() => {
            return Err(TerminalAuthorizationError::StepUpMfaRequired);
        }
        Ok(false) => return Err(TerminalAuthorizationError::Forbidden),
        Err(_) => return Err(TerminalAuthorizationError::Internal),
    }

    Ok(user)
}

pub(crate) async fn require_auth_middleware(
    State(auth): State<AuthService>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(token) = session_token_from_headers(request.headers()) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "missing auth session",
            }),
        )
            .into_response();
    };

    let user = match auth.current_user(token) {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "invalid auth session",
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
    };

    crate::observability::record_actor_email(&user.email);
    request.extensions_mut().insert(AuthenticatedUser(user));
    next.run(request).await
}
