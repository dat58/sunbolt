use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use sunbolt_protocol::{
    TerminalClientMessage, TerminalError as ProtocolTerminalError, TerminalErrorCode,
    TerminalServerMessage, TerminalSessionId, TerminalSize as ProtocolTerminalSize,
};
use sunbolt_terminal::{LocalPtySession, TerminalError, TerminalSize};
use tokio::{sync::mpsc, task};

const OUTPUT_BUFFER_SIZE: usize = 8192;
const OUTPUT_CHANNEL_CAPACITY: usize = 32;
const READ_SHUTDOWN_GRACE: Duration = Duration::from_millis(100);

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
    Router::new().route(TERMINAL_WS_PATH, get(terminal_websocket))
}

async fn terminal_websocket(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_terminal_socket)
}

async fn handle_terminal_socket(mut socket: WebSocket) {
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

    let (mut sender, mut receiver) = socket.split();
    let (output_tx, mut output_rx) = mpsc::channel(OUTPUT_CHANNEL_CAPACITY);
    let output_session = Arc::clone(&session);
    let output_session_id = session_id.clone();

    let output_reader = task::spawn_blocking(move || {
        read_pty_output(output_session, output_session_id, output_tx);
    });

    loop {
        tokio::select! {
            Some(output) = output_rx.recv() => {
                if send_split_server_message(&mut sender, output).await.is_err() {
                    break;
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(message)) => {
                        if !handle_client_frame(&session, &session_id, message, &mut sender).await {
                            break;
                        }
                    }
                    Some(Err(_)) | None => break,
                }
            }
        }
    }

    let _ = session.close();
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
            let _ = session.close();
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
            Ok(0) | Err(TerminalError::Closed) => break,
            Ok(read) => {
                let data = String::from_utf8_lossy(&buffer[..read]).into_owned();
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
                let _ = output_tx.blocking_send(TerminalServerMessage::Error {
                    session_id: Some(session_id),
                    error: protocol_error(TerminalErrorCode::TerminalUnavailable, error),
                });
                break;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::{
        component_name, parse_client_message, router, terminal_size_from_protocol, TERMINAL_WS_PATH,
    };
    use axum::{
        body::Body,
        extract::ws::Message,
        http::{Request, StatusCode},
    };
    use sunbolt_protocol::{TerminalClientMessage, TerminalSize};
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
}
