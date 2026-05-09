pub mod ha;

mod agent;
mod audit;
mod auth;
mod config;
mod error;
mod node;
mod rate_limit;
mod routes;
mod routing;
mod security;
mod state;
mod terminal;

pub use error::StartupError;
pub use routes::{
    router, try_router, ACCESS_HISTORY_PATH, AGENT_ENROLL_PATH, AGENT_HEARTBEAT_PATH,
    AGENT_TRANSPORT_WS_PATH, AUDIT_LOGS_PATH, AUTH_LOGIN_PATH, AUTH_LOGOUT_PATH, AUTH_ME_PATH,
    AUTH_MFA_STEP_UP_PATH, AUTH_TERMINAL_ACCESS_PATH, ENROLLMENT_TOKENS_PATH, HEALTH_PATH,
    NODES_PATH, TERMINAL_SESSIONS_ACTIVE_PATH, TERMINAL_SESSIONS_DETACHED_PATH, TERMINAL_WS_PATH,
};

/// Returns a stable name for the control plane component.
#[must_use]
pub fn component_name() -> String {
    format!("{} control plane", sunbolt_common::product_name())
}

#[cfg(test)]
pub(crate) use {
    agent::AgentConnectionRegistry,
    auth::authorize_terminal_request,
    config::TerminalSessionConfig,
    error::{SessionLimitError, TerminalAuthorizationError},
    rate_limit::SlidingWindowRateLimiter,
    routes::build_router,
    state::AppState,
    terminal::{
        agent_event_to_browser_message, exit_message, parse_client_message,
        terminal_size_from_protocol, TerminalSessionRegistry,
    },
};

#[cfg(test)]
mod tests {
    use super::{
        authorize_terminal_request, build_router, component_name, exit_message,
        parse_client_message, terminal_size_from_protocol, AgentConnectionRegistry, AppState,
        SessionLimitError, SlidingWindowRateLimiter, TerminalAuthorizationError,
        TerminalSessionConfig, TerminalSessionRegistry, ACCESS_HISTORY_PATH, AGENT_ENROLL_PATH,
        AGENT_HEARTBEAT_PATH, AUDIT_LOGS_PATH, AUTH_LOGIN_PATH, AUTH_LOGOUT_PATH, AUTH_ME_PATH,
        AUTH_MFA_STEP_UP_PATH, AUTH_TERMINAL_ACCESS_PATH, ENROLLMENT_TOKENS_PATH, HEALTH_PATH,
        NODES_PATH, TERMINAL_WS_PATH,
    };
    use axum::{
        body::Body,
        extract::ws::Message,
        http::{header, HeaderMap, Method, Request, StatusCode},
        response::Response,
    };
    use serde_json::{json, Value};
    use std::{process::Command, sync::Arc, time::Duration};
    use sunbolt_audit::AuditEventKind;
    use sunbolt_auth::{AuthConfig, AuthService, SESSION_COOKIE_NAME};
    use sunbolt_protocol::{
        transport::AgentTransportKind, AgentTerminalCommand, AgentTerminalEvent,
        TerminalClientMessage, TerminalError, TerminalErrorCode, TerminalReconnectToken,
        TerminalServerMessage, TerminalSessionId, TerminalSize, TerminalTransportStatus,
    };
    use sunbolt_terminal::{
        LocalPtySession, TerminalExitStatus, TerminalSessionState, TerminalSize as PtyTerminalSize,
    };
    use tokio::sync::mpsc;
    use tower::ServiceExt;

    #[test]
    fn component_name_mentions_control_plane() {
        assert_eq!(component_name(), "Sunbolt control plane");
    }

    #[test]
    fn terminal_size_from_protocol_clamps_zero_dimensions() {
        let size = terminal_size_from_protocol(TerminalSize { cols: 0, rows: 0 });

        assert_eq!(size.cols, 1);
        assert_eq!(size.rows, 1);
    }

    #[test]
    fn default_terminal_session_config_is_bounded() {
        let config = TerminalSessionConfig::from_env();

        assert!(config.max_sessions > 0);
        assert!(config.max_sessions_per_user > 0);
        assert!(config.max_sessions_per_node > 0);
        assert!(config.idle_timeout >= Duration::from_secs(60));
        assert!(config.max_duration >= Duration::from_secs(60 * 60));
    }

    #[test]
    fn terminal_session_registry_tracks_state_and_cleanup() {
        let Some(shell) = test_shell() else {
            return;
        };

        let registry = TerminalSessionRegistry::default();
        let session_id = TerminalSessionId("session-1".to_owned());
        let session = Arc::new(
            LocalPtySession::spawn_shell(shell, PtyTerminalSize::new(80, 24))
                .expect("test shell should spawn"),
        );

        let config = TerminalSessionConfig {
            max_sessions: 1,
            max_sessions_per_user: 5,
            max_sessions_per_node: 10,
            idle_timeout: Duration::from_secs(30 * 60),
            max_duration: Duration::from_secs(8 * 60 * 60),
            disconnect_grace: Duration::from_secs(30),
        };
        assert!(registry
            .insert(
                session_id.clone(),
                Arc::clone(&session),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "test@example.com".to_owned(),
                None,
            )
            .is_ok());
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.state(&session_id),
            Some(TerminalSessionState::Starting)
        );

        registry.set_state(&session_id, TerminalSessionState::Active);
        assert_eq!(
            registry.state(&session_id),
            Some(TerminalSessionState::Active)
        );
        registry.set_state(&session_id, TerminalSessionState::Detached);
        assert_eq!(
            registry.state(&session_id),
            Some(TerminalSessionState::Detached)
        );
        registry.set_state(&session_id, TerminalSessionState::Active);
        assert_eq!(
            registry.state(&session_id),
            Some(TerminalSessionState::Detached)
        );
        let reconnect_token = registry
            .reconnect_token(&session_id)
            .expect("reconnect token should be issued");
        assert!(registry
            .reattach(
                &session_id,
                &TerminalReconnectToken("wrong-token".to_owned()),
                "test@example.com"
            )
            .is_none());
        assert!(registry
            .reattach(&session_id, &reconnect_token, "test@example.com")
            .is_some());
        registry.set_state(&session_id, TerminalSessionState::Active);
        registry.set_state(&session_id, TerminalSessionState::Active);
        assert_eq!(
            registry.state(&session_id),
            Some(TerminalSessionState::Active)
        );

        assert!(registry
            .insert(
                TerminalSessionId("session-2".to_owned()),
                Arc::clone(&session),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "test@example.com".to_owned(),
                None,
            )
            .is_err());

        registry.remove(&session_id);
        assert_eq!(registry.len(), 0);
        assert!(session.is_closed());
    }

    #[test]
    fn per_user_session_limit_is_enforced() {
        let Some(shell) = test_shell() else {
            return;
        };

        let registry = TerminalSessionRegistry::default();
        let config = TerminalSessionConfig {
            max_sessions: 10,
            max_sessions_per_user: 2,
            max_sessions_per_node: 10,
            idle_timeout: Duration::from_secs(30 * 60),
            max_duration: Duration::from_secs(8 * 60 * 60),
            disconnect_grace: Duration::from_secs(30),
        };
        let spawn_session = || {
            Arc::new(
                LocalPtySession::spawn_shell(shell.clone(), PtyTerminalSize::new(80, 24))
                    .expect("test shell should spawn"),
            )
        };

        assert!(registry
            .insert(
                TerminalSessionId("s1".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "alice@example.com".to_owned(),
                None,
            )
            .is_ok());
        assert!(registry
            .insert(
                TerminalSessionId("s2".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "alice@example.com".to_owned(),
                None,
            )
            .is_ok());

        let err = registry
            .insert(
                TerminalSessionId("s3".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "alice@example.com".to_owned(),
                None,
            )
            .expect_err("user limit should be enforced");
        assert_eq!(err, SessionLimitError::PerUser);

        assert!(registry
            .insert(
                TerminalSessionId("s4".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "bob@example.com".to_owned(),
                None,
            )
            .is_ok());
    }

    #[test]
    fn per_node_session_limit_is_enforced() {
        let Some(shell) = test_shell() else {
            return;
        };

        let registry = TerminalSessionRegistry::default();
        let config = TerminalSessionConfig {
            max_sessions: 10,
            max_sessions_per_user: 10,
            max_sessions_per_node: 2,
            idle_timeout: Duration::from_secs(30 * 60),
            max_duration: Duration::from_secs(8 * 60 * 60),
            disconnect_grace: Duration::from_secs(30),
        };
        let spawn_session = || {
            Arc::new(
                LocalPtySession::spawn_shell(shell.clone(), PtyTerminalSize::new(80, 24))
                    .expect("test shell should spawn"),
            )
        };

        assert!(registry
            .insert(
                TerminalSessionId("s1".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "user1@example.com".to_owned(),
                Some("node-1".to_owned()),
            )
            .is_ok());
        assert!(registry
            .insert(
                TerminalSessionId("s2".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "user2@example.com".to_owned(),
                Some("node-1".to_owned()),
            )
            .is_ok());

        let err = registry
            .insert(
                TerminalSessionId("s3".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "user3@example.com".to_owned(),
                Some("node-1".to_owned()),
            )
            .expect_err("node limit should be enforced");
        assert_eq!(err, SessionLimitError::PerNode);

        assert!(registry
            .insert(
                TerminalSessionId("s4".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "user3@example.com".to_owned(),
                Some("node-2".to_owned()),
            )
            .is_ok());
    }

    #[test]
    fn cleanup_removes_sessions_exceeding_max_duration() {
        let Some(shell) = test_shell() else {
            return;
        };

        let registry = TerminalSessionRegistry::default();
        let config = TerminalSessionConfig {
            max_sessions: 10,
            max_sessions_per_user: 5,
            max_sessions_per_node: 10,
            idle_timeout: Duration::from_secs(30 * 60),
            max_duration: Duration::from_secs(8 * 60 * 60),
            disconnect_grace: Duration::from_secs(30),
        };
        let session = Arc::new(
            LocalPtySession::spawn_shell(shell, PtyTerminalSize::new(80, 24))
                .expect("test shell should spawn"),
        );
        assert!(registry
            .insert(
                TerminalSessionId("session-1".to_owned()),
                session,
                TerminalSize { cols: 80, rows: 24 },
                config,
                "test@example.com".to_owned(),
                None,
            )
            .is_ok());
        assert_eq!(registry.len(), 1);

        // Duration::ZERO means every session has exceeded max duration immediately
        let expired = registry.drain_expired(Duration::ZERO, Duration::from_secs(60));
        assert_eq!(expired.len(), 1);
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn exit_status_maps_to_protocol_message() {
        let message = exit_message(
            TerminalSessionId("session-1".to_owned()),
            TerminalExitStatus { code: Some(3) },
        );

        assert!(matches!(
            message,
            TerminalServerMessage::Exited {
                exit: sunbolt_protocol::TerminalExit { status: Some(3) },
                ..
            }
        ));
    }

    #[test]
    fn parse_client_message_rejects_invalid_json() {
        let error = parse_client_message(Message::Text("{".to_owned().into()))
            .expect_err("invalid JSON should be rejected");

        assert_eq!(
            error.code,
            sunbolt_protocol::TerminalErrorCode::InvalidMessage
        );
    }

    #[test]
    fn parse_client_message_accepts_start_message() {
        let message = parse_client_message(Message::Text(
            r#"{"type":"start","node_id":null,"initial_size":{"cols":80,"rows":24}}"#
                .to_owned()
                .into(),
        ))
        .expect("start message should parse");

        assert!(matches!(message, TerminalClientMessage::Start { .. }));
    }

    #[tokio::test]
    async fn terminal_route_requires_websocket_upgrade() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri(TERMINAL_WS_PATH)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_route_returns_not_found() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/missing")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn login_sets_session_cookie_and_returns_user() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_LOGIN_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "admin@example.com",
                            "password": "admin-password"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::OK);
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("set-cookie should be present")
            .to_str()
            .expect("cookie header should be utf-8");
        assert!(set_cookie.contains("sunbolt_session="));

        let body = axum::body::to_bytes(response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        assert_eq!(payload["user"]["email"], "admin@example.com");
    }

    #[tokio::test]
    async fn auth_me_requires_authentication() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(AUTH_ME_PATH)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn access_history_requires_authentication() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(ACCESS_HISTORY_PATH)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn audit_logs_capture_login_and_logout_events() {
        let router = test_router();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;

        let _failed_login_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_LOGIN_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "admin@example.com",
                            "password": "wrong-password"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        let _logout_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_LOGOUT_PATH)
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        let fresh_cookie =
            login_and_get_cookie(&router, "admin@example.com", "admin-password").await;
        let logs_response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(AUDIT_LOGS_PATH)
                    .header(header::COOKIE, fresh_cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(logs_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(logs_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        let events = payload["events"]
            .as_array()
            .expect("events should be a list");
        assert!(
            events
                .iter()
                .any(|event| event["kind"] == json!("UserLoginSuccess")),
            "expected login success event"
        );
        assert!(
            events
                .iter()
                .any(|event| event["kind"] == json!("UserLoginFailed")),
            "expected login failed event"
        );
        assert!(
            events
                .iter()
                .any(|event| event["kind"] == json!("UserLogout")),
            "expected logout event"
        );
    }

    #[tokio::test]
    async fn step_up_mfa_endpoint_records_recent_mfa_and_audit_events() {
        let router = test_router();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_MFA_STEP_UP_PATH)
                    .header(header::COOKIE, cookie.as_str())
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "factor_type": "totp"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::OK);

        let logs_response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(AUDIT_LOGS_PATH)
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        let body = axum::body::to_bytes(logs_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        let events = payload["events"]
            .as_array()
            .expect("events should be a list");
        assert!(events
            .iter()
            .any(|event| event["kind"] == json!("UserMfaChallenge")));
        assert!(events
            .iter()
            .any(|event| event["kind"] == json!("UserMfaSuccess")));
    }

    #[tokio::test]
    async fn enrollment_token_requires_authentication() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(ENROLLMENT_TOKENS_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn enrollment_token_registers_agent_once() {
        let router = test_router();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;
        let token_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(ENROLLMENT_TOKENS_PATH)
                    .header(header::COOKIE, cookie.as_str())
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"expires_in_secs": 300}).to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(token_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(token_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        let token = payload["token"].as_str().expect("token should be present");
        assert!(payload["enrollment_command"]
            .as_str()
            .expect("command should be present")
            .contains("SUNBOLT_AGENT_ENROLLMENT_TOKEN"));

        let enroll_body = json!({
            "token": token,
            "node_name": "node-a",
            "hostname": "host-a",
            "os": "linux",
            "architecture": "x86_64",
            "agent_version": "0.1.0"
        })
        .to_string();
        let enroll_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_ENROLL_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(enroll_body.clone()))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(enroll_response.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(enroll_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        assert_eq!(payload["node_id"], "node-1");

        let reused_response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_ENROLL_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(enroll_body))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(reused_response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn agent_heartbeat_marks_node_online_and_nodes_are_listed() {
        let router = test_router();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;
        let enrollment = enroll_test_agent(&router, &cookie).await;

        let heartbeat_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_HEARTBEAT_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "node_id": enrollment.node_id,
                            "credential_fingerprint": enrollment.credential_fingerprint,
                            "credential_proof": enrollment.credential_proof(),
                            "hostname": "host-a",
                            "os": "linux",
                            "architecture": "x86_64",
                            "agent_version": "0.1.0"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(heartbeat_response.status(), StatusCode::OK);

        let nodes_response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(NODES_PATH)
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(nodes_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(nodes_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        assert_eq!(payload["nodes"][0]["status"], "online");
    }

    #[tokio::test]
    async fn failed_agent_heartbeat_authentication_is_audited() {
        let (router, state) = test_router_and_state();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;
        let enrollment = enroll_test_agent(&router, &cookie).await;

        let heartbeat_response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_HEARTBEAT_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "node_id": enrollment.node_id,
                            "credential_fingerprint": enrollment.credential_fingerprint,
                            "credential_proof": "wrong-proof",
                            "hostname": "host-a",
                            "os": "linux",
                            "architecture": "x86_64",
                            "agent_version": "0.1.0"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(heartbeat_response.status(), StatusCode::UNAUTHORIZED);
        assert!(state.audit.events().iter().any(|event| {
            event.kind == AuditEventKind::AgentAuthenticationFailed
                && event.message.contains("invalid credential")
        }));
    }

    #[tokio::test]
    async fn node_details_and_revoke_are_authenticated() {
        let router = test_router();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;
        let enrollment = enroll_test_agent(&router, &cookie).await;

        let details_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("{NODES_PATH}/{}", enrollment.node_id))
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(details_response.status(), StatusCode::OK);

        let revoke_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("{NODES_PATH}/{}/revoke", enrollment.node_id))
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(revoke_response.status(), StatusCode::OK);

        let heartbeat_response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_HEARTBEAT_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "node_id": enrollment.node_id,
                            "credential_fingerprint": enrollment.credential_fingerprint,
                            "credential_proof": enrollment.credential_proof(),
                            "hostname": "host-a",
                            "os": "linux",
                            "architecture": "x86_64",
                            "agent_version": "0.1.0"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(heartbeat_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn credential_rotation_preserves_node_access_and_is_audited() {
        let (router, state) = test_router_and_state();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;
        let enrollment = enroll_test_agent(&router, &cookie).await;

        let rotate_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "{NODES_PATH}/{}/credentials/rotate",
                        enrollment.node_id
                    ))
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(rotate_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(rotate_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        let rotated_fingerprint = payload["rotation"]["credential"]["credential_fingerprint"]
            .as_str()
            .expect("rotated fingerprint should be present")
            .to_owned();
        let rotated_secret = payload["rotation"]["credential"]["credential_secret"]
            .as_str()
            .expect("rotated secret should be present")
            .to_owned();

        let old_heartbeat = post_agent_heartbeat(&router, &enrollment).await;
        assert_eq!(old_heartbeat.status(), StatusCode::OK);

        let rotated = TestEnrollment {
            node_id: enrollment.node_id.clone(),
            credential_fingerprint: rotated_fingerprint,
            credential_secret: rotated_secret,
        };
        let rotated_heartbeat = post_agent_heartbeat(&router, &rotated).await;
        assert_eq!(rotated_heartbeat.status(), StatusCode::OK);
        assert!(state.audit.events().iter().any(|event| {
            event.kind == AuditEventKind::NodeCredentialRotated
                && event.message.contains(&enrollment.node_id)
        }));
    }

    #[tokio::test]
    async fn active_node_revocation_closes_remote_sessions_and_disconnects_agent() {
        let (router, state) = test_router_and_state();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;
        let enrollment = enroll_test_agent(&router, &cookie).await;
        let session_id = TerminalSessionId("remote-1".to_owned());
        let (command_tx, mut command_rx) = mpsc::channel(4);
        let (_event_tx, event_rx) = mpsc::channel(4);
        state
            .agent_connections
            .register(&enrollment.node_id, command_tx.clone(), event_rx);
        state
            .sessions
            .insert_remote(
                session_id.clone(),
                crate::terminal::RemoteTerminalSession {
                    command_tx,
                    node_id: enrollment.node_id.clone(),
                    transport_status: TerminalTransportStatus {
                        kind: AgentTransportKind::WebSocketTlsTcp443,
                        degraded: false,
                        message: None,
                    },
                },
                TerminalSize { cols: 80, rows: 24 },
                TerminalSessionConfig {
                    max_sessions: 10,
                    max_sessions_per_user: 10,
                    max_sessions_per_node: 10,
                    idle_timeout: Duration::from_secs(30 * 60),
                    max_duration: Duration::from_secs(8 * 60 * 60),
                    disconnect_grace: Duration::from_secs(30),
                },
                "admin@example.com".to_owned(),
            )
            .expect("remote session should insert");
        state
            .sessions
            .set_state(&session_id, TerminalSessionState::Active);

        let revoke_response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("{NODES_PATH}/{}/revoke", enrollment.node_id))
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(revoke_response.status(), StatusCode::OK);
        assert_eq!(state.sessions.len(), 0);
        assert_eq!(state.agent_connections.len(), 0);
        assert!(matches!(
            command_rx.recv().await,
            Some(AgentTerminalCommand::CloseTerminal { session_id: closed })
                if closed == session_id
        ));
        let revoked_message = format!("node {} revoked", enrollment.node_id);
        assert!(state.audit.events().iter().any(|event| {
            event.kind == AuditEventKind::TerminalClosed && event.message.contains(&revoked_message)
        }));
    }

    #[test]
    fn agent_terminal_events_map_to_browser_messages() {
        let session_id = TerminalSessionId("remote-1".to_owned());
        let registry = TerminalSessionRegistry::default();
        let output = super::agent_event_to_browser_message(
            AgentTerminalEvent::TerminalOutput {
                session_id: session_id.clone(),
                data: "hello\n".to_owned(),
            },
            &sunbolt_protocol::NodeId("node-1".to_owned()),
            &registry,
            None,
        );

        assert_eq!(
            output,
            TerminalServerMessage::Output {
                session_id: session_id.clone(),
                sequence: 0,
                data: "hello\n".to_owned(),
            }
        );

        let error = super::agent_event_to_browser_message(
            AgentTerminalEvent::TerminalError {
                session_id: session_id.clone(),
                error: TerminalError {
                    code: TerminalErrorCode::TerminalUnavailable,
                    message: "agent disconnected".to_owned(),
                },
            },
            &sunbolt_protocol::NodeId("node-1".to_owned()),
            &registry,
            None,
        );

        assert!(matches!(
            error,
            TerminalServerMessage::Error {
                session_id: Some(_),
                ..
            }
        ));
    }

    #[tokio::test]
    async fn agent_connection_registry_tracks_active_channel() {
        let registry = AgentConnectionRegistry::default();
        let (command_tx, mut command_rx) = mpsc::channel(1);
        let (_event_tx, event_rx) = mpsc::channel(1);
        registry.register("node-1", command_tx, event_rx);

        let connection = registry
            .connection("node-1")
            .expect("agent connection should be registered");
        connection
            .command_tx
            .send(AgentTerminalCommand::CloseTerminal {
                session_id: TerminalSessionId("remote-1".to_owned()),
            })
            .await
            .expect("command should send");

        assert_eq!(registry.len(), 1);
        assert!(matches!(
            command_rx.recv().await,
            Some(AgentTerminalCommand::CloseTerminal { .. })
        ));

        registry.disconnect("node-1");
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn viewer_cannot_open_terminal() {
        let auth = test_auth_service();
        let (_, token) = auth
            .login("viewer@example.com", "viewer-password")
            .expect("viewer should log in");

        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{SESSION_COOKIE_NAME}={token}")
                .parse()
                .expect("cookie header should parse"),
        );

        let error =
            authorize_terminal_request(&auth, &headers).expect_err("viewer should be forbidden");
        assert_eq!(error, TerminalAuthorizationError::Forbidden);
    }

    #[test]
    fn terminal_authorization_requires_session_cookie() {
        let headers = HeaderMap::new();
        let auth = test_auth_service();

        let error = authorize_terminal_request(&auth, &headers)
            .expect_err("missing auth should be rejected");
        assert_eq!(error, TerminalAuthorizationError::Unauthorized);
    }

    #[test]
    fn terminal_authorization_requires_recent_step_up_mfa() {
        let auth = test_auth_service();
        let (_, token) = auth
            .login("admin@example.com", "admin-password")
            .expect("admin should log in");
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{SESSION_COOKIE_NAME}={token}")
                .parse()
                .expect("cookie header should parse"),
        );

        let error = authorize_terminal_request(&auth, &headers)
            .expect_err("missing step-up MFA should be rejected");
        assert_eq!(error, TerminalAuthorizationError::StepUpMfaRequired);

        auth.record_mfa_success(&token)
            .expect("MFA success should record");
        let user = authorize_terminal_request(&auth, &headers)
            .expect("recent step-up MFA should allow terminal");
        assert_eq!(user.email, "admin@example.com");
    }

    fn test_shell() -> Option<String> {
        for candidate in ["/bin/sh", "/usr/bin/sh"] {
            if Command::new(candidate)
                .arg("-c")
                .arg("exit 0")
                .status()
                .is_ok()
            {
                return Some(candidate.to_owned());
            }
        }

        None
    }

    fn test_router() -> axum::Router {
        test_router_with_origins(vec![])
    }

    fn test_router_with_origins(allowed_origins: Vec<String>) -> axum::Router {
        let state = test_state_with_origins(allowed_origins);
        build_router(state)
    }

    fn test_router_and_state() -> (axum::Router, AppState) {
        let state = test_state_with_origins(vec![]);
        (build_router(state.clone()), state)
    }

    fn test_state_with_origins(allowed_origins: Vec<String>) -> AppState {
        let mut state = AppState::development_with_auth(test_auth_service());
        state.login_rate_limiter = SlidingWindowRateLimiter::new(Duration::from_secs(60), 100);
        state.terminal_rate_limiter = SlidingWindowRateLimiter::new(Duration::from_secs(60), 100);
        state.allowed_origins = allowed_origins;
        state
    }

    fn test_auth_service() -> AuthService {
        let auth = AuthService::new(AuthConfig {
            session_ttl: Duration::from_secs(60 * 60),
            recent_mfa_ttl: Duration::from_secs(10 * 60),
            secure_cookie: false,
            require_step_up_mfa_for_terminal: true,
            bootstrap_admin: false,
            admin_email: "unused@example.com".to_owned(),
            admin_password: "unused".to_owned(),
        });
        auth.upsert_user(
            "admin@example.com",
            "admin-password",
            sunbolt_auth::UserRole::Admin,
        )
        .expect("admin should be created");
        auth.upsert_user(
            "viewer@example.com",
            "viewer-password",
            sunbolt_auth::UserRole::Viewer,
        )
        .expect("viewer should be created");
        auth
    }

    struct TestEnrollment {
        node_id: String,
        credential_fingerprint: String,
        credential_secret: String,
    }

    impl TestEnrollment {
        fn credential_proof(&self) -> String {
            crate::agent::credential_proof(&self.node_id, &self.credential_secret)
        }
    }

    async fn enroll_test_agent(router: &axum::Router, cookie: &str) -> TestEnrollment {
        let token_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(ENROLLMENT_TOKENS_PATH)
                    .header(header::COOKIE, cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"expires_in_secs": 300}).to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(token_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(token_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        let token = payload["token"].as_str().expect("token should be present");

        let enroll_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_ENROLL_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "token": token,
                            "node_name": "node-a",
                            "hostname": "host-a",
                            "os": "linux",
                            "architecture": "x86_64",
                            "agent_version": "0.1.0"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(enroll_response.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(enroll_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");

        TestEnrollment {
            node_id: payload["node_id"]
                .as_str()
                .expect("node id should be present")
                .to_owned(),
            credential_fingerprint: payload["credential_fingerprint"]
                .as_str()
                .expect("credential fingerprint should be present")
                .to_owned(),
            credential_secret: payload["credential_secret"]
                .as_str()
                .expect("credential secret should be present")
                .to_owned(),
        }
    }

    async fn post_agent_heartbeat(router: &axum::Router, enrollment: &TestEnrollment) -> Response {
        router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_HEARTBEAT_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "node_id": enrollment.node_id,
                            "credential_fingerprint": enrollment.credential_fingerprint,
                            "credential_proof": enrollment.credential_proof(),
                            "hostname": "host-a",
                            "os": "linux",
                            "architecture": "x86_64",
                            "agent_version": "0.1.0"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond")
    }

    async fn login_and_get_cookie(router: &axum::Router, email: &str, password: &str) -> String {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_LOGIN_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": email,
                            "password": password
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::OK);

        response
            .headers()
            .get(header::SET_COOKIE)
            .expect("set-cookie should be present")
            .to_str()
            .expect("cookie should be utf-8")
            .split(';')
            .next()
            .expect("cookie should contain a token")
            .to_owned()
    }

    #[tokio::test]
    async fn security_headers_are_present_on_all_responses() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri(AUTH_ME_PATH)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        let csp = response
            .headers()
            .get("content-security-policy")
            .expect("CSP header should be present");
        assert!(csp
            .to_str()
            .expect("CSP header should be utf-8")
            .contains("default-src 'self'"));
    }

    #[tokio::test]
    async fn health_endpoint_reports_ready() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri(HEALTH_PATH)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");

        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["component"], "sunbolt-control");
    }

    #[tokio::test]
    async fn login_rate_limit_blocks_excessive_attempts() {
        let mut state = AppState::development_with_auth(test_auth_service());
        state.login_rate_limiter = SlidingWindowRateLimiter::new(Duration::from_secs(60), 2);
        state.terminal_rate_limiter = SlidingWindowRateLimiter::new(Duration::from_secs(60), 100);
        let router = build_router(state);

        let make_request = || {
            Request::builder()
                .method(Method::POST)
                .uri(AUTH_LOGIN_PATH)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"email": "admin@example.com", "password": "wrong"}).to_string(),
                ))
                .expect("request should build")
        };

        let r1 = router
            .clone()
            .oneshot(make_request())
            .await
            .expect("should respond");
        assert_eq!(r1.status(), StatusCode::UNAUTHORIZED);

        let r2 = router
            .clone()
            .oneshot(make_request())
            .await
            .expect("should respond");
        assert_eq!(r2.status(), StatusCode::UNAUTHORIZED);

        let r3 = router
            .clone()
            .oneshot(make_request())
            .await
            .expect("should respond");
        assert_eq!(r3.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn login_preflight_returns_credentialed_cors_headers() {
        let router = test_router_with_origins(vec!["http://localhost:8080".to_owned()]);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri(AUTH_LOGIN_PATH)
                    .header("origin", "http://localhost:8080")
                    .header("access-control-request-method", "POST")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&header::HeaderValue::from_static("http://localhost:8080"))
        );
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            Some(&header::HeaderValue::from_static("true"))
        );
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_HEADERS),
            Some(&header::HeaderValue::from_static("content-type"))
        );
    }

    #[tokio::test]
    async fn cross_origin_login_is_rejected_when_origin_list_is_configured() {
        let router = test_router_with_origins(vec!["http://localhost:3000".to_owned()]);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_LOGIN_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("origin", "http://evil.example.com")
                    .body(Body::from(
                        json!({"email": "admin@example.com", "password": "admin-password"})
                            .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn allowed_origin_login_succeeds() {
        let router = test_router_with_origins(vec!["http://localhost:3000".to_owned()]);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_LOGIN_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("origin", "http://localhost:3000")
                    .body(Body::from(
                        json!({"email": "admin@example.com", "password": "admin-password"})
                            .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&header::HeaderValue::from_static("http://localhost:3000"))
        );
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            Some(&header::HeaderValue::from_static("true"))
        );
    }

    #[tokio::test]
    async fn auth_me_succeeds_cross_origin_with_session_cookie() {
        let router = test_router_with_origins(vec!["http://localhost:8080".to_owned()]);
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(AUTH_ME_PATH)
                    .header("origin", "http://localhost:8080")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&header::HeaderValue::from_static("http://localhost:8080"))
        );
    }

    #[tokio::test]
    async fn terminal_access_reports_step_up_requirement_before_success() {
        let router = test_router_with_origins(vec!["http://localhost:8080".to_owned()]);
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(AUTH_TERMINAL_ACCESS_PATH)
                    .header("origin", "http://localhost:8080")
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        assert_eq!(
            payload.get("error").and_then(Value::as_str),
            Some("step-up MFA is required before opening a terminal")
        );

        let mfa_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_MFA_STEP_UP_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("origin", "http://localhost:8080")
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::from(json!({ "factor_type": "totp" }).to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(mfa_response.status(), StatusCode::OK);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(AUTH_TERMINAL_ACCESS_PATH)
                    .header("origin", "http://localhost:8080")
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        assert_eq!(payload.get("accepted").and_then(Value::as_bool), Some(true));
    }

    #[tokio::test]
    async fn wildcard_allowed_origins_reflects_request_origin() {
        let router = test_router_with_origins(vec!["*".to_owned()]);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_LOGIN_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("origin", "http://127.0.0.1:8080")
                    .body(Body::from(
                        json!({"email": "admin@example.com", "password": "admin-password"})
                            .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&header::HeaderValue::from_static("http://127.0.0.1:8080"))
        );
    }
}
