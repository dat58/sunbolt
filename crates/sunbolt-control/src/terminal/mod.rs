pub(crate) mod session_registry;

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Extension, Path, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use futures_util::{SinkExt, StreamExt};
use sunbolt_audit::{AuditEventInput, AuditEventKind, AuditLog};
use sunbolt_auth::{Permission, User};
use sunbolt_protocol::{
    AgentTerminalCommand, AgentTerminalEvent, NodeId, TerminalClientMessage,
    TerminalError as ProtocolTerminalError, TerminalErrorCode, TerminalExit,
    TerminalReconnectToken, TerminalServerMessage, TerminalSessionId,
    TerminalSize as ProtocolTerminalSize, TerminalTransportStatus,
};
use sunbolt_terminal::{
    LocalPtySession, TerminalError, TerminalExitStatus, TerminalSessionState, TerminalSize,
};
use tokio::{
    sync::{broadcast, mpsc},
    task,
};
use tracing::{field, info, info_span};

pub(crate) use session_registry::{
    RemoteTerminalSession, TerminalBackend, TerminalSessionRegistry, TerminalSessionSummary,
};

use crate::{
    auth::{authorize_terminal_request, AuthenticatedUser},
    config::TerminalSessionConfig,
    error::{ErrorResponse, TerminalAuthorizationError},
    routing::{NodeRoute, NodeRouter, RouteRequest},
    security,
    state::AppState,
};

const OUTPUT_BUFFER_SIZE: usize = 8192;
const READ_SHUTDOWN_GRACE: Duration = Duration::from_millis(100);
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, serde::Serialize)]
pub(crate) struct TerminalSessionListResponse {
    sessions: Vec<TerminalSessionSummary>,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct TerminalTerminateResponse {
    session_id: String,
    terminated: bool,
}

pub(crate) async fn list_active_terminal_sessions(
    State(state): State<AppState>,
    Extension(AuthenticatedUser(user)): Extension<AuthenticatedUser>,
) -> impl IntoResponse {
    crate::observability::record_actor_email(&user.email);
    Json(TerminalSessionListResponse {
        sessions: state.sessions.list_sessions_for_actor(
            &user.email,
            &[
                TerminalSessionState::Active,
                TerminalSessionState::Reattaching,
            ],
        ),
    })
}

pub(crate) async fn list_detached_terminal_sessions(
    State(state): State<AppState>,
    Extension(AuthenticatedUser(user)): Extension<AuthenticatedUser>,
) -> impl IntoResponse {
    crate::observability::record_actor_email(&user.email);
    Json(TerminalSessionListResponse {
        sessions: state
            .sessions
            .list_sessions_for_actor(&user.email, &[TerminalSessionState::Detached]),
    })
}

pub(crate) async fn terminate_terminal_session(
    State(state): State<AppState>,
    Extension(AuthenticatedUser(user)): Extension<AuthenticatedUser>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let session_id = TerminalSessionId(session_id);
    let actor_email = user.email.clone();
    crate::observability::record_actor_email(&actor_email);
    crate::observability::record_session_id(&session_id.0);
    if let Some(node_id) = state.sessions.node_id(&session_id) {
        crate::observability::record_node_id(&node_id);
    }
    info_span!(
        "terminal.terminate",
        actor_email = %actor_email,
        session_id = %session_id.0,
    )
    .in_scope(|| info!("terminal terminate requested"));
    if !state.sessions.owner_matches(&session_id, &user.email) {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "terminal session was not found",
            }),
        )
            .into_response();
    }
    if !authorize_session_terminate(&state, &user, &session_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "terminal terminate is not permitted for this session",
            }),
        )
            .into_response();
    }

    let backend = state.sessions.terminate(&session_id);
    let Some(backend) = backend else {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "terminal session was not found",
            }),
        )
            .into_response();
    };
    if let TerminalBackend::Remote { command_tx } = backend {
        let _ = command_tx
            .send(AgentTerminalCommand::CloseTerminal {
                session_id: session_id.clone(),
            })
            .await;
    }
    state.audit.record(AuditEventInput {
        kind: AuditEventKind::TerminalTerminated,
        actor_email: Some(actor_email),
        message: format!("terminal session {} explicitly terminated", session_id.0),
    });
    Json(TerminalTerminateResponse {
        session_id: session_id.0,
        terminated: true,
    })
    .into_response()
}

pub(crate) async fn terminal_websocket(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if !security::is_allowed_origin(&headers, &state.allowed_origins) {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "cross-origin websocket request rejected",
            }),
        )
            .into_response();
    }

    let user = match authorize_terminal_request(&state.auth, &headers) {
        Ok(user) => user,
        Err(error) => {
            let status = error.status_code();
            state.audit.record(AuditEventInput {
                kind: if error == TerminalAuthorizationError::StepUpMfaRequired {
                    AuditEventKind::UserMfaChallenge
                } else {
                    AuditEventKind::TerminalFailed
                },
                actor_email: None,
                message: error.message().to_owned(),
            });
            return (
                status,
                Json(ErrorResponse {
                    error: error.message(),
                }),
            )
                .into_response();
        }
    };
    crate::observability::record_actor_email(&user.email);

    if !state.terminal_rate_limiter.check_and_record(&user.email) {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalFailed,
            actor_email: Some(user.email.clone()),
            message: "terminal creation rate limit exceeded".to_owned(),
        });
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorResponse {
                error: "terminal creation rate limit exceeded",
            }),
        )
            .into_response();
    }

    ws.on_upgrade(move |socket| handle_terminal_socket(socket, state, user))
        .into_response()
}

#[allow(clippy::too_many_lines)]
async fn handle_terminal_socket(mut socket: WebSocket, state: AppState, user: User) {
    let actor_email = user.email.clone();
    let Some(handshake) = receive_start_message(&mut socket).await else {
        return;
    };

    let start = match handshake {
        TerminalHandshake::Start(start) => start,
        TerminalHandshake::Reattach {
            session_id,
            reconnect_token,
        } => {
            handle_terminal_reattach(socket, state, user, session_id, reconnect_token).await;
            return;
        }
    };

    let initial_size = terminal_size_from_protocol(start.initial_size);
    let session_id = next_session_id();
    crate::observability::record_actor_email(&actor_email);
    crate::observability::record_session_id(&session_id.0);

    if let Some(node_id) = start.node_id {
        crate::observability::record_node_id(&node_id.0);
        handle_remote_terminal_socket(
            socket,
            state,
            actor_email,
            node_id,
            session_id,
            start.initial_size,
        )
        .await;
        return;
    }

    info_span!(
        "terminal.open",
        actor_email = %actor_email,
        session_id = %session_id.0,
        node_id = field::Empty,
    )
    .in_scope(|| info!("local terminal open requested"));

    let session = match LocalPtySession::spawn_default_shell(initial_size) {
        Ok(session) => Arc::new(session),
        Err(error) => {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::TerminalFailed,
                actor_email: Some(actor_email),
                message: format!("terminal spawn failed: {error}"),
            });
            let _ = send_server_message(
                &mut socket,
                TerminalServerMessage::Error {
                    session_id: Some(session_id),
                    error: protocol_error(TerminalErrorCode::TerminalUnavailable, error),
                },
            )
            .await;
            return;
        }
    };

    let output_tx = match state.sessions.insert(
        session_id.clone(),
        Arc::clone(&session),
        start.initial_size,
        state.terminal_config,
        actor_email.clone(),
        None,
    ) {
        Ok(tx) => tx,
        Err(limit_error) => {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::TerminalFailed,
                actor_email: Some(actor_email),
                message: limit_error.message().to_owned(),
            });
            let _ = session.close();
            let _ = send_server_message(
                &mut socket,
                TerminalServerMessage::Error {
                    session_id: Some(session_id),
                    error: protocol_error_text(
                        TerminalErrorCode::TerminalUnavailable,
                        limit_error.message(),
                    ),
                },
            )
            .await;
            return;
        }
    };

    if send_server_message(
        &mut socket,
        TerminalServerMessage::Started {
            session_id: session_id.clone(),
            node_id: start.node_id,
            size: start.initial_size,
            reconnect_token: state.sessions.reconnect_token(&session_id),
            transport_status: None,
        },
    )
    .await
    .is_err()
    {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalFailed,
            actor_email: Some(actor_email),
            message: "failed to send terminal started message".to_owned(),
        });
        let _ = session.close();
        return;
    }

    state.audit.record(AuditEventInput {
        kind: AuditEventKind::TerminalOpened,
        actor_email: Some(actor_email.clone()),
        message: format!("terminal session {} opened", session_id.0),
    });

    state
        .sessions
        .set_state(&session_id, TerminalSessionState::Active);

    let (mut sender, mut receiver) = socket.split();
    let mut output_rx = output_tx.subscribe();
    let output_session = Arc::clone(&session);
    let output_session_id = session_id.clone();
    let output_registry = state.sessions.clone();

    let output_reader = task::spawn_blocking(move || {
        read_pty_output(
            output_registry,
            output_session,
            output_session_id,
            output_tx,
        );
    });

    let mut idle_check = tokio::time::interval(IDLE_CHECK_INTERVAL);
    let mut terminal_failed = false;

    loop {
        tokio::select! {
            output = output_rx.recv() => {
                let output = match output {
                    Ok(output) => output,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                state.sessions.touch(&session_id);
                let is_terminal_exit = matches!(output, TerminalServerMessage::Exited { .. });
                if is_terminal_exit {
                    state.sessions.set_state(&session_id, TerminalSessionState::Terminated);
                }
                if let TerminalServerMessage::Error { error, .. } = &output {
                    terminal_failed = true;
                    state.audit.record(AuditEventInput {
                        kind: AuditEventKind::TerminalFailed,
                        actor_email: Some(actor_email.clone()),
                        message: format!("terminal stream error: {}", error.message),
                    });
                }
                if send_split_server_message(&mut sender, output).await.is_err() {
                    state.sessions.detach(&session_id);
                    audit_terminal_detached(&state.audit, &actor_email, &session_id, "browser websocket disconnected");
                    break;
                }
                if is_terminal_exit {
                    break;
                }
            }
            incoming = receiver.next() => {
                if let Some(Ok(message)) = incoming {
                    state.sessions.touch(&session_id);
                    if !handle_client_frame(&state.sessions, &state.audit, &actor_email, &session, &session_id, message, &mut sender).await {
                        break;
                    }
                } else {
                    state.sessions.detach(&session_id);
                    audit_terminal_detached(&state.audit, &actor_email, &session_id, "browser websocket disconnected");
                    break;
                }
            }
            _ = idle_check.tick() => {
                if state.sessions.is_idle(&session_id, state.terminal_config.idle_timeout) {
                    state.sessions.set_state(&session_id, TerminalSessionState::Terminating);
                    let _ = send_split_server_message(
                        &mut sender,
                        TerminalServerMessage::Error {
                            session_id: Some(session_id.clone()),
                            error: protocol_error_text(
                                TerminalErrorCode::TerminalUnavailable,
                                "terminal session idle timeout reached",
                            ),
                        },
                    )
                    .await;
                    break;
                } else if state
                    .sessions
                    .is_exceeded_max_duration(&session_id, state.terminal_config.max_duration)
                {
                    state.sessions.set_state(&session_id, TerminalSessionState::Terminating);
                    let _ = send_split_server_message(
                        &mut sender,
                        TerminalServerMessage::Error {
                            session_id: Some(session_id.clone()),
                            error: protocol_error_text(
                                TerminalErrorCode::TerminalUnavailable,
                                "terminal session exceeded maximum allowed duration",
                            ),
                        },
                    )
                    .await;
                    break;
                }
            }
        }
    }

    if matches!(
        state.sessions.state(&session_id),
        Some(TerminalSessionState::Detached)
    ) {
        let _ = tokio::time::timeout(READ_SHUTDOWN_GRACE, output_reader).await;
        return;
    }
    state
        .sessions
        .set_state(&session_id, TerminalSessionState::Terminating);
    state.sessions.remove(&session_id);
    let _ = tokio::time::timeout(READ_SHUTDOWN_GRACE, output_reader).await;

    if !terminal_failed {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalClosed,
            actor_email: Some(actor_email),
            message: format!("terminal session {} closed", session_id.0),
        });
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_terminal_reattach(
    socket: WebSocket,
    state: AppState,
    user: User,
    session_id: TerminalSessionId,
    reconnect_token: TerminalReconnectToken,
) {
    let actor_email = user.email.clone();
    crate::observability::record_actor_email(&actor_email);
    crate::observability::record_session_id(&session_id.0);
    if let Some(node_id) = state.sessions.node_id(&session_id) {
        crate::observability::record_node_id(&node_id);
    }
    info_span!(
        "terminal.reattach",
        actor_email = %actor_email,
        session_id = %session_id.0,
    )
    .in_scope(|| info!("terminal reattach requested"));
    if !authorize_session_reattach(&state, &user, &session_id) {
        let mut socket = socket;
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    TerminalErrorCode::Forbidden,
                    "terminal reattach is not permitted for this session",
                ),
            },
        )
        .await;
        return;
    }

    let Some(target) = state
        .sessions
        .reattach(&session_id, &reconnect_token, &actor_email)
    else {
        let mut socket = socket;
        let error_code = terminal_missing_or_expired_code(&state, &session_id);
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    error_code,
                    "terminal session was not found or is no longer reattachable",
                ),
            },
        )
        .await;
        return;
    };

    let (mut sender, mut receiver) = socket.split();
    let _ = send_split_server_message(
        &mut sender,
        TerminalServerMessage::Reattached {
            session_id: session_id.clone(),
            node_id: target.node_id.clone().map(NodeId),
            size: target.size,
            reconnect_token: Some(target.reconnect_token),
            transport_status: target.transport_status.clone(),
        },
    )
    .await;
    for replay in target.replay {
        let _ = send_split_server_message(&mut sender, replay).await;
    }
    state
        .sessions
        .set_state(&session_id, TerminalSessionState::Active);
    state.audit.record(AuditEventInput {
        kind: AuditEventKind::TerminalReattached,
        actor_email: Some(actor_email.clone()),
        message: format!("terminal session {} reattached", session_id.0),
    });

    let mut idle_check = tokio::time::interval(IDLE_CHECK_INTERVAL);
    let mut terminal_failed = false;
    let session = match target.backend {
        TerminalBackend::Local(session) => session,
        TerminalBackend::Remote { command_tx } => {
            handle_remote_attached_socket(
                sender,
                receiver,
                state,
                actor_email,
                session_id,
                target.output_rx,
                command_tx,
            )
            .await;
            return;
        }
    };
    let mut output_rx = target.output_rx;

    loop {
        tokio::select! {
            output = output_rx.recv() => {
                let output = match output {
                    Ok(output) => output,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                state.sessions.touch(&session_id);
                let is_terminal_exit = matches!(output, TerminalServerMessage::Exited { .. });
                if is_terminal_exit {
                    state.sessions.set_state(&session_id, TerminalSessionState::Terminated);
                }
                if let TerminalServerMessage::Error { error, .. } = &output {
                    terminal_failed = true;
                    state.audit.record(AuditEventInput {
                        kind: AuditEventKind::TerminalFailed,
                        actor_email: Some(actor_email.clone()),
                        message: format!("terminal stream error: {}", error.message),
                    });
                }
                if send_split_server_message(&mut sender, output).await.is_err() {
                    state.sessions.detach(&session_id);
                    audit_terminal_detached(&state.audit, &actor_email, &session_id, "browser websocket disconnected");
                    break;
                }
                if is_terminal_exit {
                    break;
                }
            }
            incoming = receiver.next() => {
                if let Some(Ok(message)) = incoming {
                    state.sessions.touch(&session_id);
                    if !handle_client_frame(&state.sessions, &state.audit, &actor_email, &session, &session_id, message, &mut sender).await {
                        break;
                    }
                } else {
                    state.sessions.detach(&session_id);
                    audit_terminal_detached(&state.audit, &actor_email, &session_id, "browser websocket disconnected");
                    break;
                }
            }
            _ = idle_check.tick() => {
                if state.sessions.is_idle(&session_id, state.terminal_config.idle_timeout) {
                    state.sessions.set_state(&session_id, TerminalSessionState::Terminating);
                    let _ = send_split_server_message(
                        &mut sender,
                        TerminalServerMessage::Error {
                            session_id: Some(session_id.clone()),
                            error: protocol_error_text(
                                TerminalErrorCode::TerminalUnavailable,
                                "terminal session idle timeout reached",
                            ),
                        },
                    )
                    .await;
                    break;
                } else if state
                    .sessions
                    .is_exceeded_max_duration(&session_id, state.terminal_config.max_duration)
                {
                    state.sessions.set_state(&session_id, TerminalSessionState::Terminating);
                    let _ = send_split_server_message(
                        &mut sender,
                        TerminalServerMessage::Error {
                            session_id: Some(session_id.clone()),
                            error: protocol_error_text(
                                TerminalErrorCode::TerminalUnavailable,
                                "terminal session exceeded maximum allowed duration",
                            ),
                        },
                    )
                    .await;
                    break;
                }
            }
        }
    }

    if matches!(
        state.sessions.state(&session_id),
        Some(TerminalSessionState::Detached)
    ) {
        return;
    }

    state
        .sessions
        .set_state(&session_id, TerminalSessionState::Terminating);
    state.sessions.remove(&session_id);

    if !terminal_failed {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalClosed,
            actor_email: Some(actor_email),
            message: format!("terminal session {} closed", session_id.0),
        });
    }
}

pub(crate) fn spawn_session_cleanup_worker(
    sessions: TerminalSessionRegistry,
    config: TerminalSessionConfig,
    audit: AuditLog,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
        interval.tick().await;
        loop {
            interval.tick().await;
            let expired = sessions.drain_expired(config.max_duration, config.idle_timeout);
            for (session_id, actor_email, output_tx, backend) in expired {
                let _ = output_tx.send(TerminalServerMessage::Error {
                    session_id: Some(session_id.clone()),
                    error: protocol_error_text(
                        TerminalErrorCode::SessionExpired,
                        "terminal session expired",
                    ),
                });
                if let TerminalBackend::Remote { command_tx } = backend {
                    let _ = command_tx
                        .send(AgentTerminalCommand::CloseTerminal {
                            session_id: session_id.clone(),
                        })
                        .await;
                }
                audit.record(AuditEventInput {
                    kind: AuditEventKind::TerminalClosed,
                    actor_email: Some(actor_email),
                    message: format!("terminal session {} expired", session_id.0),
                });
            }
        }
    });
}

fn authorize_session_reattach(
    state: &AppState,
    user: &User,
    session_id: &TerminalSessionId,
) -> bool {
    if !state.sessions.owner_matches(session_id, &user.email) {
        return false;
    }
    let Some(node_id) = state.sessions.node_id(session_id) else {
        return true;
    };
    if state
        .node_enrollment
        .node_details(&node_id)
        .is_some_and(|node| matches!(node.status, crate::node::NodeStatus::Revoked))
    {
        return false;
    }
    state
        .auth
        .user_has_node_permission(user, &node_id, Permission::TERMINAL_REATTACH)
        .unwrap_or(false)
}

fn authorize_session_terminate(
    state: &AppState,
    user: &User,
    session_id: &TerminalSessionId,
) -> bool {
    let Some(node_id) = state.sessions.node_id(session_id) else {
        return true;
    };
    state
        .auth
        .user_has_node_permission(user, &node_id, Permission::TERMINAL_CLOSE)
        .unwrap_or(false)
}

fn terminal_missing_or_expired_code(
    state: &AppState,
    session_id: &TerminalSessionId,
) -> TerminalErrorCode {
    if matches!(
        state.sessions.state(session_id),
        Some(TerminalSessionState::Expired | TerminalSessionState::Terminated)
    ) {
        TerminalErrorCode::SessionExpired
    } else {
        TerminalErrorCode::SessionNotFound
    }
}

fn audit_terminal_detached(
    audit: &AuditLog,
    actor_email: &str,
    session_id: &TerminalSessionId,
    reason: &str,
) {
    info_span!(
        "terminal.detach",
        actor_email = %actor_email,
        session_id = %session_id.0,
        reason = %reason,
    )
    .in_scope(|| info!("terminal detached"));
    audit.record(AuditEventInput {
        kind: AuditEventKind::TerminalDetached,
        actor_email: Some(actor_email.to_owned()),
        message: format!("terminal session {} detached: {reason}", session_id.0),
    });
}

struct StartTerminal {
    node_id: Option<sunbolt_protocol::NodeId>,
    initial_size: ProtocolTerminalSize,
}

enum TerminalHandshake {
    Start(StartTerminal),
    Reattach {
        session_id: TerminalSessionId,
        reconnect_token: TerminalReconnectToken,
    },
}

async fn receive_start_message(socket: &mut WebSocket) -> Option<TerminalHandshake> {
    match socket.recv().await {
        Some(Ok(message)) => match parse_client_message(message) {
            Ok(TerminalClientMessage::Start {
                node_id,
                initial_size,
            }) => Some(TerminalHandshake::Start(StartTerminal {
                node_id,
                initial_size,
            })),
            Ok(TerminalClientMessage::Reattach {
                session_id,
                reconnect_token,
            }) => Some(TerminalHandshake::Reattach {
                session_id,
                reconnect_token,
            }),
            Ok(_) => {
                let _ = send_server_message(
                    socket,
                    TerminalServerMessage::Error {
                        session_id: None,
                        error: protocol_error_text(
                            TerminalErrorCode::InvalidMessage,
                            "first terminal message must be start or reattach",
                        ),
                    },
                )
                .await;
                None
            }
            Err(error) => {
                let _ = send_server_message(
                    socket,
                    TerminalServerMessage::Error {
                        session_id: None,
                        error,
                    },
                )
                .await;
                None
            }
        },
        Some(Err(_)) | None => None,
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_remote_terminal_socket(
    mut socket: WebSocket,
    state: AppState,
    actor_email: String,
    node_id: NodeId,
    session_id: TerminalSessionId,
    initial_size: ProtocolTerminalSize,
) {
    let node_id_text = node_id.0.clone();
    crate::observability::record_actor_email(&actor_email);
    crate::observability::record_node_id(&node_id_text);
    crate::observability::record_session_id(&session_id.0);
    info_span!(
        "terminal.open",
        actor_email = %actor_email,
        node_id = %node_id_text,
        session_id = %session_id.0,
    )
    .in_scope(|| info!("remote terminal open requested"));
    if !state.node_enrollment.node_is_online(&node_id_text) {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalFailed,
            actor_email: Some(actor_email),
            message: format!("remote terminal requested for unavailable node {node_id_text}"),
        });
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    TerminalErrorCode::TerminalUnavailable,
                    "agent node is not online",
                ),
            },
        )
        .await;
        return;
    }

    let Ok(route) = state.node_router.select_route(RouteRequest {
        target_node_id: node_id.clone(),
        direct_agent_connected: state.agent_connections.connection(&node_id_text).is_some(),
        relay_candidates: state
            .agent_connections
            .connected_node_ids_except(&node_id_text),
    }) else {
        info_span!(
            "route.failed",
            node_id = %node_id_text,
            session_id = %session_id.0,
        )
        .in_scope(|| info!("no healthy route available"));
        crate::audit::record_route_failed(
            &state.audit,
            &actor_email,
            &node_id_text,
            &session_id.0,
            None,
            "no healthy route is available",
        );
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalFailed,
            actor_email: Some(actor_email),
            message: format!("remote terminal requested without healthy route {node_id_text}"),
        });
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    TerminalErrorCode::TerminalUnavailable,
                    "no healthy route is available for the agent node",
                ),
            },
        )
        .await;
        return;
    };
    let route_id = route.route_id();
    crate::observability::record_route_id(&route_id);
    info_span!(
        "route.selected",
        node_id = %node_id_text,
        session_id = %session_id.0,
        route_id = %route_id,
    )
    .in_scope(|| info!("terminal route selected"));
    crate::audit::record_route_selected(
        &state.audit,
        &actor_email,
        &node_id_text,
        &session_id.0,
        &route_id,
    );

    let NodeRoute::DirectAgent { .. } = &route else {
        state.node_router.record_failure(&route);
        info_span!(
            "route.failed",
            node_id = %node_id_text,
            session_id = %session_id.0,
            route_id = %route_id,
        )
        .in_scope(|| info!("selected route is not executable"));
        crate::audit::record_route_failed(
            &state.audit,
            &actor_email,
            &node_id_text,
            &session_id.0,
            Some(&route_id),
            "selected relay route is not executable for terminal streams",
        );
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalFailed,
            actor_email: Some(actor_email),
            message: format!(
                "relay route selected for {node_id_text}, but relay execution is not enabled"
            ),
        });
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    TerminalErrorCode::TerminalUnavailable,
                    "relay routing is selected but not enabled for terminal streams",
                ),
            },
        )
        .await;
        return;
    };

    let Some(connection) = state.agent_connections.connection(&node_id_text) else {
        state.node_router.record_failure(&route);
        info_span!(
            "route.failed",
            node_id = %node_id_text,
            session_id = %session_id.0,
            route_id = %route_id,
        )
        .in_scope(|| info!("selected route has no active agent connection"));
        crate::audit::record_route_failed(
            &state.audit,
            &actor_email,
            &node_id_text,
            &session_id.0,
            Some(&route_id),
            "selected route has no active agent connection",
        );
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalFailed,
            actor_email: Some(actor_email),
            message: format!("remote terminal requested without agent channel {node_id_text}"),
        });
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    TerminalErrorCode::TerminalUnavailable,
                    "agent connection is not active",
                ),
            },
        )
        .await;
        return;
    };

    if connection
        .command_tx
        .send(AgentTerminalCommand::StartTerminal {
            session_id: session_id.clone(),
            size: initial_size,
        })
        .await
        .is_err()
    {
        state.agent_connections.disconnect(&node_id_text);
        state.node_router.record_failure(&route);
        info_span!(
            "route.failed",
            node_id = %node_id_text,
            session_id = %session_id.0,
            route_id = %route_id,
        )
        .in_scope(|| info!("selected route dropped before terminal start"));
        crate::audit::record_route_failed(
            &state.audit,
            &actor_email,
            &node_id_text,
            &session_id.0,
            Some(&route_id),
            "selected route dropped before terminal start",
        );
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    TerminalErrorCode::TerminalUnavailable,
                    "agent connection dropped",
                ),
            },
        )
        .await;
        return;
    }

    let output_tx = match state.sessions.insert_remote(
        session_id.clone(),
        RemoteTerminalSession {
            command_tx: connection.command_tx.clone(),
            node_id: node_id_text.clone(),
            transport_status: connection.terminal_transport_status(),
        },
        initial_size,
        state.terminal_config,
        actor_email.clone(),
    ) {
        Ok(tx) => tx,
        Err(limit_error) => {
            let _ = connection
                .command_tx
                .send(AgentTerminalCommand::CloseTerminal {
                    session_id: session_id.clone(),
                })
                .await;
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::TerminalFailed,
                actor_email: Some(actor_email),
                message: limit_error.message().to_owned(),
            });
            let _ = send_server_message(
                &mut socket,
                TerminalServerMessage::Error {
                    session_id: Some(session_id),
                    error: protocol_error_text(
                        TerminalErrorCode::TerminalUnavailable,
                        limit_error.message(),
                    ),
                },
            )
            .await;
            return;
        }
    };
    crate::observability::record_transport_id(&connection.transport_id.0);

    state.node_router.record_success(&route);

    let (mut sender, mut receiver) = socket.split();
    let mut event_rx = connection.event_rx.lock().await;
    let mut terminal_opened = false;
    let mut terminal_failed = false;

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                let Some(event) = event else {
                    terminal_failed = true;
                    let _ = send_split_server_message(
                        &mut sender,
                        TerminalServerMessage::Error {
                            session_id: Some(session_id.clone()),
                            error: protocol_error_text(
                                TerminalErrorCode::TerminalUnavailable,
                                "agent disconnected during terminal session",
                            ),
                        },
                    )
                    .await;
                    break;
                };
                let message = agent_event_to_browser_message(
                    event,
                    &node_id,
                    &state.sessions,
                    Some(connection.terminal_transport_status()),
                );
                if matches!(message, TerminalServerMessage::Started { .. }) {
                    terminal_opened = true;
                    state.sessions.set_state(&session_id, TerminalSessionState::Active);
                    state.audit.record(AuditEventInput {
                        kind: AuditEventKind::TerminalOpened,
                        actor_email: Some(actor_email.clone()),
                        message: format!("remote terminal session {} opened on {node_id_text}", session_id.0),
                    });
                }
                if matches!(message, TerminalServerMessage::Error { .. }) {
                    terminal_failed = true;
                }
                let is_terminal_exit = matches!(message, TerminalServerMessage::Exited { .. });
                if !matches!(message, TerminalServerMessage::Output { .. }) {
                    state.sessions.remember_server_message(message.clone());
                }
                let _ = output_tx.send(message.clone());
                if send_split_server_message(&mut sender, message).await.is_err() {
                    state.sessions.detach(&session_id);
                    audit_terminal_detached(&state.audit, &actor_email, &session_id, "browser websocket disconnected");
                    break;
                }
                if is_terminal_exit {
                    state.sessions.remove(&session_id);
                    break;
                }
            }
            incoming = receiver.next() => {
                if let Some(Ok(message)) = incoming {
                    if !handle_remote_client_frame(&state, &actor_email, &connection.command_tx, &session_id, message, &mut sender).await {
                        break;
                    }
                } else {
                    state.sessions.detach(&session_id);
                    audit_terminal_detached(&state.audit, &actor_email, &session_id, "browser websocket disconnected");
                    break;
                }
            }
        }
    }

    if matches!(
        state.sessions.state(&session_id),
        Some(TerminalSessionState::Detached)
    ) {
        return;
    }
    let _ = state.sessions.terminate(&session_id);
    let _ = connection
        .command_tx
        .send(AgentTerminalCommand::CloseTerminal {
            session_id: session_id.clone(),
        })
        .await;
    if terminal_failed {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalFailed,
            actor_email: Some(actor_email),
            message: format!("remote terminal session {} failed", session_id.0),
        });
    } else if terminal_opened {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalClosed,
            actor_email: Some(actor_email),
            message: format!("remote terminal session {} closed", session_id.0),
        });
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_remote_client_frame(
    state: &AppState,
    actor_email: &str,
    command_tx: &mpsc::Sender<AgentTerminalCommand>,
    active_session_id: &TerminalSessionId,
    message: Message,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    let message = match parse_client_message(message) {
        Ok(message) => message,
        Err(error) => {
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Error {
                    session_id: Some(active_session_id.clone()),
                    error,
                },
            )
            .await;
            return true;
        }
    };

    let command = match message {
        TerminalClientMessage::Input { session_id, data } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            AgentTerminalCommand::WriteInput { session_id, data }
        }
        TerminalClientMessage::Resize { session_id, size } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            AgentTerminalCommand::ResizeTerminal { session_id, size }
        }
        TerminalClientMessage::Close { session_id }
        | TerminalClientMessage::Terminate { session_id } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            state
                .sessions
                .set_state(active_session_id, TerminalSessionState::Terminating);
            info_span!(
                "terminal.terminate",
                actor_email = %actor_email,
                session_id = %active_session_id.0,
            )
            .in_scope(|| info!("remote terminal terminate requested"));
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::TerminalTerminated,
                actor_email: Some(actor_email.to_owned()),
                message: format!(
                    "remote terminal session {} explicitly terminated",
                    active_session_id.0
                ),
            });
            let _ = command_tx
                .send(AgentTerminalCommand::CloseTerminal { session_id })
                .await;
            return false;
        }
        TerminalClientMessage::Detach { session_id } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            state.sessions.detach(active_session_id);
            audit_terminal_detached(
                &state.audit,
                actor_email,
                active_session_id,
                "user detached remote terminal session",
            );
            let _ =
                send_split_server_message(sender, TerminalServerMessage::Detached { session_id })
                    .await;
            return false;
        }
        TerminalClientMessage::Reattach { session_id, .. } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Error {
                    session_id: Some(session_id),
                    error: protocol_error_text(
                        TerminalErrorCode::InvalidMessage,
                        "remote terminal reattach is not available yet",
                    ),
                },
            )
            .await;
            return true;
        }
        TerminalClientMessage::Ping { nonce } => {
            let _ = send_split_server_message(sender, TerminalServerMessage::Pong { nonce }).await;
            return true;
        }
        TerminalClientMessage::Start { .. } => {
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Error {
                    session_id: Some(active_session_id.clone()),
                    error: protocol_error_text(
                        TerminalErrorCode::InvalidMessage,
                        "terminal session is already started",
                    ),
                },
            )
            .await;
            return true;
        }
    };

    if command_tx.send(command).await.is_err() {
        let _ = send_split_server_message(
            sender,
            TerminalServerMessage::Error {
                session_id: Some(active_session_id.clone()),
                error: protocol_error_text(
                    TerminalErrorCode::TerminalUnavailable,
                    "agent connection dropped",
                ),
            },
        )
        .await;
        return false;
    }

    true
}

#[allow(clippy::too_many_lines)]
async fn handle_remote_attached_socket(
    mut sender: futures_util::stream::SplitSink<WebSocket, Message>,
    mut receiver: futures_util::stream::SplitStream<WebSocket>,
    state: AppState,
    actor_email: String,
    session_id: TerminalSessionId,
    mut output_rx: broadcast::Receiver<TerminalServerMessage>,
    command_tx: mpsc::Sender<AgentTerminalCommand>,
) {
    remote_attached_loop(
        &state,
        &actor_email,
        &session_id,
        &command_tx,
        &mut sender,
        &mut receiver,
        &mut output_rx,
    )
    .await;
}

async fn remote_attached_loop(
    state: &AppState,
    actor_email: &str,
    session_id: &TerminalSessionId,
    command_tx: &mpsc::Sender<AgentTerminalCommand>,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    receiver: &mut futures_util::stream::SplitStream<WebSocket>,
    output_rx: &mut broadcast::Receiver<TerminalServerMessage>,
) {
    loop {
        tokio::select! {
            output = output_rx.recv() => {
                let output = match output {
                    Ok(output) => output,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                state.sessions.touch(session_id);
                let is_terminal_exit = matches!(output, TerminalServerMessage::Exited { .. });
                if send_split_server_message(sender, output).await.is_err() {
                    state.sessions.detach(session_id);
                    audit_terminal_detached(&state.audit, actor_email, session_id, "browser websocket disconnected");
                    break;
                }
                if is_terminal_exit {
                    state.sessions.remove(session_id);
                    break;
                }
            }
            incoming = receiver.next() => {
                if let Some(Ok(message)) = incoming {
                    state.sessions.touch(session_id);
                    if !handle_remote_client_frame(
                        state,
                        actor_email,
                        command_tx,
                        session_id,
                        message,
                        sender,
                    )
                    .await
                    {
                        break;
                    }
                } else {
                    state.sessions.detach(session_id);
                    audit_terminal_detached(&state.audit, actor_email, session_id, "browser websocket disconnected");
                    break;
                }
            }
        }
    }
}

pub(crate) fn agent_event_to_browser_message(
    event: AgentTerminalEvent,
    node_id: &NodeId,
    registry: &TerminalSessionRegistry,
    transport_status: Option<TerminalTransportStatus>,
) -> TerminalServerMessage {
    match event {
        AgentTerminalEvent::TerminalStarted { session_id, size } => {
            TerminalServerMessage::Started {
                session_id,
                node_id: Some(node_id.clone()),
                size,
                reconnect_token: None,
                transport_status,
            }
        }
        AgentTerminalEvent::TerminalOutput { session_id, data } => registry
            .record_output(&session_id, data.clone())
            .unwrap_or(TerminalServerMessage::Output {
                session_id,
                sequence: 0,
                data,
            }),
        AgentTerminalEvent::TerminalExited { session_id, exit } => {
            TerminalServerMessage::Exited { session_id, exit }
        }
        AgentTerminalEvent::TerminalError { session_id, error } => TerminalServerMessage::Error {
            session_id: Some(session_id),
            error,
        },
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_client_frame(
    registry: &TerminalSessionRegistry,
    audit: &AuditLog,
    actor_email: &str,
    session: &LocalPtySession,
    active_session_id: &TerminalSessionId,
    message: Message,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    let message = match parse_client_message(message) {
        Ok(message) => message,
        Err(error) => {
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Error {
                    session_id: Some(active_session_id.clone()),
                    error,
                },
            )
            .await;
            return true;
        }
    };

    match message {
        TerminalClientMessage::Input { session_id, data } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            if let Err(error) = session.write_input(data.as_bytes()) {
                let _ = send_split_server_message(
                    sender,
                    TerminalServerMessage::Error {
                        session_id: Some(active_session_id.clone()),
                        error: protocol_error(TerminalErrorCode::TerminalUnavailable, error),
                    },
                )
                .await;
            }
            true
        }
        TerminalClientMessage::Resize { session_id, size } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            registry.set_size(active_session_id, size);
            if let Err(error) = session.resize(terminal_size_from_protocol(size)) {
                let _ = send_split_server_message(
                    sender,
                    TerminalServerMessage::Error {
                        session_id: Some(active_session_id.clone()),
                        error: protocol_error(TerminalErrorCode::TerminalUnavailable, error),
                    },
                )
                .await;
            }
            true
        }
        TerminalClientMessage::Close { session_id }
        | TerminalClientMessage::Terminate { session_id } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            registry.set_state(active_session_id, TerminalSessionState::Terminating);
            info_span!(
                "terminal.terminate",
                actor_email = %actor_email,
                session_id = %active_session_id.0,
            )
            .in_scope(|| info!("local terminal terminate requested"));
            audit.record(AuditEventInput {
                kind: AuditEventKind::TerminalTerminated,
                actor_email: Some(actor_email.to_owned()),
                message: format!(
                    "terminal session {} explicitly terminated",
                    active_session_id.0
                ),
            });
            let _ = session.close();
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Exited {
                    session_id: active_session_id.clone(),
                    exit: TerminalExit { status: None },
                },
            )
            .await;
            false
        }
        TerminalClientMessage::Detach { session_id } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            audit_terminal_detached(
                audit,
                actor_email,
                active_session_id,
                "user detached terminal session",
            );
            detach_local_terminal(registry, active_session_id, sender).await
        }
        TerminalClientMessage::Reattach { session_id, .. } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            reattach_local_terminal(registry, active_session_id, sender).await
        }
        TerminalClientMessage::Ping { nonce } => {
            let _ = send_split_server_message(sender, TerminalServerMessage::Pong { nonce }).await;
            true
        }
        TerminalClientMessage::Start { .. } => {
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Error {
                    session_id: Some(active_session_id.clone()),
                    error: protocol_error_text(
                        TerminalErrorCode::InvalidMessage,
                        "terminal session is already started",
                    ),
                },
            )
            .await;
            true
        }
    }
}

async fn detach_local_terminal(
    registry: &TerminalSessionRegistry,
    active_session_id: &TerminalSessionId,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    registry.set_state(active_session_id, TerminalSessionState::Detached);
    let _ = send_split_server_message(
        sender,
        TerminalServerMessage::Detached {
            session_id: active_session_id.clone(),
        },
    )
    .await;
    false
}

async fn reattach_local_terminal(
    registry: &TerminalSessionRegistry,
    active_session_id: &TerminalSessionId,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    registry.set_state(active_session_id, TerminalSessionState::Reattaching);
    registry.set_state(active_session_id, TerminalSessionState::Active);
    let _ = send_split_server_message(
        sender,
        TerminalServerMessage::Reattached {
            session_id: active_session_id.clone(),
            node_id: None,
            size: ProtocolTerminalSize { cols: 80, rows: 24 },
            reconnect_token: registry.reconnect_token(active_session_id),
            transport_status: None,
        },
    )
    .await;
    registry.set_state(active_session_id, TerminalSessionState::Active);
    true
}

async fn session_id_matches(
    received: &TerminalSessionId,
    active: &TerminalSessionId,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    if received == active {
        return true;
    }

    let _ = send_split_server_message(
        sender,
        TerminalServerMessage::Error {
            session_id: Some(active.clone()),
            error: protocol_error_text(
                TerminalErrorCode::SessionNotFound,
                "terminal session id does not match this connection",
            ),
        },
    )
    .await;

    false
}

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn read_pty_output(
    registry: TerminalSessionRegistry,
    session: Arc<LocalPtySession>,
    session_id: TerminalSessionId,
    output_tx: broadcast::Sender<TerminalServerMessage>,
) {
    let mut buffer = [0_u8; OUTPUT_BUFFER_SIZE];

    loop {
        if session.is_closed() {
            break;
        }

        match session.read_output(&mut buffer) {
            Ok(0) | Err(TerminalError::Closed) => {
                break;
            }
            Ok(read) => {
                let data = String::from_utf8_lossy(&buffer[..read]).into_owned();
                // The bounded channel is the temporary backpressure strategy:
                // this blocking send slows PTY reads when the WebSocket writer
                // cannot keep up, instead of buffering terminal output without
                // limit.
                if let Some(message) = registry.record_output(&session_id, data) {
                    let _ = output_tx.send(message);
                }
            }
            Err(error) => {
                if let Ok(Some(exit)) = session.try_wait_exit() {
                    let _ = output_tx.send(exit_message(session_id.clone(), exit));
                } else {
                    let _ = output_tx.send(TerminalServerMessage::Error {
                        session_id: Some(session_id.clone()),
                        error: protocol_error(TerminalErrorCode::TerminalUnavailable, error),
                    });
                }
                break;
            }
        }
    }

    if let Ok(Some(exit)) = session.wait_exit() {
        let _ = output_tx.send(exit_message(session_id, exit));
    }
}

pub(crate) fn exit_message(
    session_id: TerminalSessionId,
    exit: TerminalExitStatus,
) -> TerminalServerMessage {
    TerminalServerMessage::Exited {
        session_id,
        exit: TerminalExit { status: exit.code },
    }
}

pub(crate) fn parse_client_message(
    message: Message,
) -> Result<TerminalClientMessage, ProtocolTerminalError> {
    match message {
        Message::Text(text) => serde_json::from_str(&text).map_err(|error| {
            protocol_error_text(
                TerminalErrorCode::InvalidMessage,
                format!("invalid terminal message JSON: {error}"),
            )
        }),
        Message::Binary(_) => Err(protocol_error_text(
            TerminalErrorCode::InvalidMessage,
            "binary terminal messages are not supported",
        )),
        Message::Close(_) => Err(protocol_error_text(
            TerminalErrorCode::InvalidMessage,
            "terminal websocket closed",
        )),
        Message::Ping(_) | Message::Pong(_) => Err(protocol_error_text(
            TerminalErrorCode::InvalidMessage,
            "websocket control frames are not terminal protocol messages",
        )),
    }
}

pub(crate) async fn send_server_message(
    socket: &mut WebSocket,
    message: TerminalServerMessage,
) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(serialize_server_message(&message).into()))
        .await
}

pub(crate) async fn send_split_server_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: TerminalServerMessage,
) -> Result<(), axum::Error> {
    sender
        .send(Message::Text(serialize_server_message(&message).into()))
        .await
}

pub(crate) fn serialize_server_message(message: &TerminalServerMessage) -> String {
    serde_json::to_string(message).expect("terminal server messages should serialize")
}

pub(crate) fn protocol_error(
    code: TerminalErrorCode,
    error: impl std::error::Error,
) -> ProtocolTerminalError {
    protocol_error_text(code, error.to_string())
}

pub(crate) fn protocol_error_text(
    code: TerminalErrorCode,
    message: impl Into<String>,
) -> ProtocolTerminalError {
    ProtocolTerminalError {
        code,
        message: message.into(),
    }
}

pub(crate) fn terminal_size_from_protocol(size: ProtocolTerminalSize) -> TerminalSize {
    let cols = size.cols.max(1);
    let rows = size.rows.max(1);
    TerminalSize { cols, rows }
}

pub(crate) fn next_session_id() -> TerminalSessionId {
    let id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    TerminalSessionId(format!("local-{id}"))
}

#[cfg(test)]
mod tests {
    use axum::extract::ws::Message;
    use serde_json::json;
    use sunbolt_protocol::{
        TerminalClientMessage, TerminalErrorCode, TerminalReconnectToken, TerminalServerMessage,
        TerminalSessionId, TerminalSize as ProtocolTerminalSize,
    };

    use super::{parse_client_message, serialize_server_message};

    #[test]
    fn serializes_started_message_with_existing_shape() {
        let message = TerminalServerMessage::Started {
            session_id: TerminalSessionId("local-1".to_owned()),
            node_id: None,
            size: ProtocolTerminalSize { cols: 80, rows: 24 },
            reconnect_token: Some(TerminalReconnectToken("token-1".to_owned())),
            transport_status: None,
        };

        let value: serde_json::Value = serde_json::from_str(&serialize_server_message(&message))
            .expect("server message should serialize as JSON");

        assert_eq!(
            value,
            json!({
                "type": "started",
                "session_id": "local-1",
                "node_id": null,
                "size": {
                    "cols": 80,
                    "rows": 24
                },
                "reconnect_token": "token-1",
                "transport_status": null
            })
        );
    }

    #[test]
    fn serializes_error_message_with_existing_shape() {
        let message = TerminalServerMessage::Error {
            session_id: Some(TerminalSessionId("local-1".to_owned())),
            error: super::protocol_error_text(
                TerminalErrorCode::TerminalUnavailable,
                "agent connection dropped",
            ),
        };

        let value: serde_json::Value = serde_json::from_str(&serialize_server_message(&message))
            .expect("server message should serialize as JSON");

        assert_eq!(
            value,
            json!({
                "type": "error",
                "session_id": "local-1",
                "error": {
                    "code": "terminal_unavailable",
                    "message": "agent connection dropped"
                }
            })
        );
    }

    #[test]
    fn parses_reattach_message_with_existing_shape() {
        let message = parse_client_message(Message::Text(
            r#"{"type":"reattach","session_id":"local-1","reconnect_token":"token-1"}"#
                .to_owned()
                .into(),
        ))
        .expect("reattach message should parse");

        assert_eq!(
            message,
            TerminalClientMessage::Reattach {
                session_id: TerminalSessionId("local-1".to_owned()),
                reconnect_token: TerminalReconnectToken("token-1".to_owned()),
            }
        );
    }
}
