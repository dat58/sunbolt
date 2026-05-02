use std::{
    collections::HashMap,
    env,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use sunbolt_protocol::{
    TerminalClientMessage, TerminalError as ProtocolTerminalError, TerminalErrorCode, TerminalExit,
    TerminalServerMessage, TerminalSessionId, TerminalSize as ProtocolTerminalSize,
};
use sunbolt_terminal::{
    LocalPtySession, TerminalError, TerminalExitStatus, TerminalSessionState, TerminalSize,
};
use tokio::{sync::mpsc, task};

const OUTPUT_BUFFER_SIZE: usize = 8192;
const OUTPUT_CHANNEL_CAPACITY: usize = 32;
const READ_SHUTDOWN_GRACE: Duration = Duration::from_millis(100);
const DEFAULT_MAX_TERMINAL_SESSIONS: usize = 16;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(5);

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

/// WebSocket path for browser terminal connections.
pub const TERMINAL_WS_PATH: &str = "/terminal/ws";

/// Returns a stable name for the control plane component.
#[must_use]
pub fn component_name() -> String {
    format!("{} control plane", sunbolt_common::product_name())
}

/// Builds the control-plane router.
pub fn router() -> Router {
    Router::new()
        .route(TERMINAL_WS_PATH, get(terminal_websocket))
        .with_state(AppState::from_env())
}

#[derive(Clone)]
struct AppState {
    sessions: TerminalSessionRegistry,
    terminal_config: TerminalSessionConfig,
}

impl AppState {
    fn from_env() -> Self {
        Self {
            sessions: TerminalSessionRegistry::default(),
            terminal_config: TerminalSessionConfig::from_env(),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct TerminalSessionConfig {
    max_sessions: usize,
    idle_timeout: Duration,
}

impl TerminalSessionConfig {
    fn from_env() -> Self {
        Self {
            max_sessions: env_usize("SUNBOLT_MAX_TERMINAL_SESSIONS")
                .unwrap_or(DEFAULT_MAX_TERMINAL_SESSIONS),
            idle_timeout: env_duration_secs("SUNBOLT_TERMINAL_IDLE_TIMEOUT_SECS")
                .unwrap_or(DEFAULT_IDLE_TIMEOUT),
        }
    }
}

#[derive(Clone, Default)]
struct TerminalSessionRegistry {
    inner: Arc<Mutex<HashMap<TerminalSessionId, TrackedTerminalSession>>>,
}

struct TrackedTerminalSession {
    session: Arc<LocalPtySession>,
    state: TerminalSessionState,
    last_activity: Instant,
}

impl TerminalSessionRegistry {
    fn insert(
        &self,
        session_id: TerminalSessionId,
        session: Arc<LocalPtySession>,
        max_sessions: usize,
    ) -> bool {
        let Ok(mut sessions) = self.inner.lock() else {
            return false;
        };
        if sessions.len() >= max_sessions {
            return false;
        }

        sessions.insert(
            session_id,
            TrackedTerminalSession {
                session,
                state: TerminalSessionState::Starting,
                last_activity: Instant::now(),
            },
        );
        true
    }

    fn set_state(&self, session_id: &TerminalSessionId, state: TerminalSessionState) {
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                session.state = state;
            }
        }
    }

    fn touch(&self, session_id: &TerminalSessionId) {
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                session.last_activity = Instant::now();
            }
        }
    }

    fn is_idle(&self, session_id: &TerminalSessionId, timeout: Duration) -> bool {
        let Ok(sessions) = self.inner.lock() else {
            return true;
        };
        sessions
            .get(session_id)
            .is_none_or(|session| session.last_activity.elapsed() >= timeout)
    }

    fn remove(&self, session_id: &TerminalSessionId) {
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.remove(session_id) {
                let _ = session.session.close();
            }
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().map_or(0, |sessions| sessions.len())
    }

    #[cfg(test)]
    fn state(&self, session_id: &TerminalSessionId) -> Option<TerminalSessionState> {
        self.inner
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(session_id).map(|session| session.state))
    }
}

impl Drop for TerminalSessionRegistry {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) != 1 {
            return;
        }

        if let Ok(mut sessions) = self.inner.lock() {
            for (_, session) in sessions.drain() {
                let _ = session.session.close();
            }
        }
    }
}

async fn terminal_websocket(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal_socket(socket, state))
}

#[allow(clippy::too_many_lines)]
async fn handle_terminal_socket(mut socket: WebSocket, state: AppState) {
    let Some(start) = receive_start_message(&mut socket).await else {
        return;
    };

    let initial_size = terminal_size_from_protocol(start.initial_size);
    let session_id = next_session_id();

    let session = match LocalPtySession::spawn_default_shell(initial_size) {
        Ok(session) => Arc::new(session),
        Err(error) => {
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

    if !state.sessions.insert(
        session_id.clone(),
        Arc::clone(&session),
        state.terminal_config.max_sessions,
    ) {
        let _ = session.close();
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    TerminalErrorCode::TerminalUnavailable,
                    "maximum terminal session count reached",
                ),
            },
        )
        .await;
        return;
    }

    if send_server_message(
        &mut socket,
        TerminalServerMessage::Started {
            session_id: session_id.clone(),
            node_id: start.node_id,
            size: start.initial_size,
        },
    )
    .await
    .is_err()
    {
        let _ = session.close();
        return;
    }

    state
        .sessions
        .set_state(&session_id, TerminalSessionState::Active);

    let (mut sender, mut receiver) = socket.split();
    let (output_tx, mut output_rx) = mpsc::channel(OUTPUT_CHANNEL_CAPACITY);
    let output_session = Arc::clone(&session);
    let output_session_id = session_id.clone();

    let output_reader = task::spawn_blocking(move || {
        read_pty_output(output_session, output_session_id, output_tx);
    });

    let mut idle_check = tokio::time::interval(IDLE_CHECK_INTERVAL);

    loop {
        tokio::select! {
            Some(output) = output_rx.recv() => {
                state.sessions.touch(&session_id);
                if matches!(output, TerminalServerMessage::Exited { .. }) {
                    state.sessions.set_state(&session_id, TerminalSessionState::Closed);
                }
                if send_split_server_message(&mut sender, output).await.is_err() {
                    break;
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(message)) => {
                        state.sessions.touch(&session_id);
                        if !handle_client_frame(&state.sessions, &session, &session_id, message, &mut sender).await {
                            break;
                        }
                    }
                    Some(Err(_)) | None => break,
                }
            }
            _ = idle_check.tick() => {
                if state.sessions.is_idle(&session_id, state.terminal_config.idle_timeout) {
                    state.sessions.set_state(&session_id, TerminalSessionState::Closing);
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
                }
            }
        }
    }

    state
        .sessions
        .set_state(&session_id, TerminalSessionState::Closing);
    state.sessions.remove(&session_id);
    let _ = tokio::time::timeout(READ_SHUTDOWN_GRACE, output_reader).await;
}

struct StartTerminal {
    node_id: Option<sunbolt_protocol::NodeId>,
    initial_size: ProtocolTerminalSize,
}

async fn receive_start_message(socket: &mut WebSocket) -> Option<StartTerminal> {
    match socket.recv().await {
        Some(Ok(message)) => match parse_client_message(message) {
            Ok(TerminalClientMessage::Start {
                node_id,
                initial_size,
            }) => Some(StartTerminal {
                node_id,
                initial_size,
            }),
            Ok(_) => {
                let _ = send_server_message(
                    socket,
                    TerminalServerMessage::Error {
                        session_id: None,
                        error: protocol_error_text(
                            TerminalErrorCode::InvalidMessage,
                            "first terminal message must be start",
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

async fn handle_client_frame(
    registry: &TerminalSessionRegistry,
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
        TerminalClientMessage::Close { session_id } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            registry.set_state(active_session_id, TerminalSessionState::Closing);
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
fn read_pty_output(
    session: Arc<LocalPtySession>,
    session_id: TerminalSessionId,
    output_tx: mpsc::Sender<TerminalServerMessage>,
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
                if output_tx
                    .blocking_send(TerminalServerMessage::Output {
                        session_id: session_id.clone(),
                        data,
                    })
                    .is_err()
                {
                    break;
                }
            }
            Err(error) => {
                if let Ok(Some(exit)) = session.try_wait_exit() {
                    let _ = output_tx.blocking_send(exit_message(session_id.clone(), exit));
                } else {
                    let _ = output_tx.blocking_send(TerminalServerMessage::Error {
                        session_id: Some(session_id.clone()),
                        error: protocol_error(TerminalErrorCode::TerminalUnavailable, error),
                    });
                }
                break;
            }
        }
    }

    if let Ok(Some(exit)) = session.wait_exit() {
        let _ = output_tx.blocking_send(exit_message(session_id, exit));
    }
}

fn exit_message(session_id: TerminalSessionId, exit: TerminalExitStatus) -> TerminalServerMessage {
    TerminalServerMessage::Exited {
        session_id,
        exit: TerminalExit { status: exit.code },
    }
}

fn parse_client_message(message: Message) -> Result<TerminalClientMessage, ProtocolTerminalError> {
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

async fn send_server_message(
    socket: &mut WebSocket,
    message: TerminalServerMessage,
) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(serialize_server_message(&message).into()))
        .await
}

async fn send_split_server_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: TerminalServerMessage,
) -> Result<(), axum::Error> {
    sender
        .send(Message::Text(serialize_server_message(&message).into()))
        .await
}

fn serialize_server_message(message: &TerminalServerMessage) -> String {
    serde_json::to_string(message).expect("terminal server messages should serialize")
}

fn protocol_error(code: TerminalErrorCode, error: impl std::error::Error) -> ProtocolTerminalError {
    protocol_error_text(code, error.to_string())
}

fn protocol_error_text(
    code: TerminalErrorCode,
    message: impl Into<String>,
) -> ProtocolTerminalError {
    ProtocolTerminalError {
        code,
        message: message.into(),
    }
}

fn terminal_size_from_protocol(size: ProtocolTerminalSize) -> TerminalSize {
    let cols = size.cols.max(1);
    let rows = size.rows.max(1);
    TerminalSize { cols, rows }
}

fn next_session_id() -> TerminalSessionId {
    let id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    TerminalSessionId(format!("local-{id}"))
}

fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.parse().ok()
}

fn env_duration_secs(name: &str) -> Option<Duration> {
    env_usize(name).and_then(|seconds| u64::try_from(seconds).ok().map(Duration::from_secs))
}

#[cfg(test)]
mod tests {
    use super::{
        component_name, exit_message, parse_client_message, router, terminal_size_from_protocol,
        TerminalSessionConfig, TerminalSessionRegistry, TERMINAL_WS_PATH,
    };
    use axum::{
        body::Body,
        extract::ws::Message,
        http::{Request, StatusCode},
    };
    use std::{process::Command, sync::Arc, time::Duration};
    use sunbolt_protocol::{
        TerminalClientMessage, TerminalServerMessage, TerminalSessionId, TerminalSize,
    };
    use sunbolt_terminal::{
        LocalPtySession, TerminalExitStatus, TerminalSessionState, TerminalSize as PtyTerminalSize,
    };
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
        assert!(config.idle_timeout >= Duration::from_secs(60));
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

        assert!(registry.insert(session_id.clone(), Arc::clone(&session), 1));
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

        assert!(!registry.insert(
            TerminalSessionId("session-2".to_owned()),
            Arc::clone(&session),
            1
        ));

        registry.remove(&session_id);
        assert_eq!(registry.len(), 0);
        assert!(session.is_closed());
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
        let response = router()
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
        let response = router()
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
}
