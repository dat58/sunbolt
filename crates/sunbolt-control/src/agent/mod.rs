use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sunbolt_audit::{AuditEventInput, AuditEventKind};
use sunbolt_protocol::{
    transport::{
        AgentTransportClientHello, AgentTransportEnvelope, AgentTransportError,
        AgentTransportErrorCode, AgentTransportHeartbeat, AgentTransportId, AgentTransportKind,
        AgentTransportMessageId, AgentTransportPayload, AgentTransportReconnectPolicy,
        AgentTransportResumeDecision, AgentTransportServerHello,
    },
    AgentTerminalCommand, AgentTerminalEvent, NodeId, PROTOCOL_VERSION,
};
use tokio::{
    sync::{mpsc, Mutex as AsyncMutex},
    time::{self, Instant},
};
use tracing::{info, warn};

use crate::{
    error::{AgentTransportConnectionError, NodeConnectionError},
    node::NodeView,
    state::AppState,
};

const AGENT_TRANSPORT_CHANNEL_CAPACITY: usize = 128;
const AGENT_TRANSPORT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const AGENT_TRANSPORT_LIVENESS_TIMEOUT: Duration = Duration::from_secs(90);
const AGENT_TRANSPORT_MAX_IN_FLIGHT_MESSAGES: u32 = 128;

#[derive(Debug, Clone)]
pub(crate) struct RegisteredAgentConnection {
    pub(crate) command_tx: mpsc::Sender<AgentTerminalCommand>,
    pub(crate) event_rx: Arc<AsyncMutex<mpsc::Receiver<AgentTerminalEvent>>>,
    pub(crate) transport_id: AgentTransportId,
}

#[derive(Clone, Default)]
pub(crate) struct AgentConnectionRegistry {
    inner: Arc<Mutex<HashMap<String, RegisteredAgentConnection>>>,
}

impl AgentConnectionRegistry {
    #[cfg(test)]
    pub(crate) fn register(
        &self,
        node_id: impl Into<String>,
        command_tx: mpsc::Sender<AgentTerminalCommand>,
        event_rx: mpsc::Receiver<AgentTerminalEvent>,
    ) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                node_id.into(),
                RegisteredAgentConnection {
                    command_tx,
                    event_rx: Arc::new(AsyncMutex::new(event_rx)),
                    transport_id: AgentTransportId("test-transport".to_owned()),
                },
            );
    }

    pub(crate) fn register_transport(
        &self,
        node_id: impl Into<String>,
        transport_id: AgentTransportId,
        _transport_kind: AgentTransportKind,
        command_tx: mpsc::Sender<AgentTerminalCommand>,
        event_rx: mpsc::Receiver<AgentTerminalEvent>,
    ) -> AgentConnectionRegistration {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let node_id = node_id.into();
        let replaced_existing = inner
            .insert(
                node_id,
                RegisteredAgentConnection {
                    command_tx,
                    event_rx: Arc::new(AsyncMutex::new(event_rx)),
                    transport_id,
                },
            )
            .is_some();

        if replaced_existing {
            AgentConnectionRegistration::ReplacedExisting
        } else {
            AgentConnectionRegistration::Registered
        }
    }

    pub(crate) fn connection(&self, node_id: &str) -> Option<RegisteredAgentConnection> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(node_id)
            .cloned()
    }

    pub(crate) fn disconnect(&self, node_id: &str) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(node_id);
    }

    pub(crate) fn disconnect_transport(
        &self,
        node_id: &str,
        transport_id: &AgentTransportId,
    ) -> bool {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner
            .get(node_id)
            .is_some_and(|connection| &connection.transport_id == transport_id)
        {
            inner.remove(node_id);
            true
        } else {
            false
        }
    }

    pub(crate) fn connected_node_ids_except(&self, node_id: &str) -> Vec<NodeId> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .filter(|candidate| candidate.as_str() != node_id)
            .cloned()
            .map(NodeId)
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum AgentConnectionRegistration {
    Registered,
    ReplacedExisting,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AgentEnrollmentRequest {
    pub(crate) token: String,
    pub(crate) node_name: String,
    pub(crate) hostname: String,
    pub(crate) os: String,
    pub(crate) architecture: String,
    pub(crate) agent_version: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentEnrollmentResponse {
    pub(crate) node_id: String,
    pub(crate) credential_fingerprint: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AgentHeartbeatRequest {
    pub(crate) node_id: String,
    pub(crate) credential_fingerprint: String,
    pub(crate) hostname: String,
    pub(crate) os: String,
    pub(crate) architecture: String,
    pub(crate) agent_version: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentHeartbeatResponse {
    pub(crate) accepted: bool,
    pub(crate) node: NodeView,
}

pub(crate) fn credential_fingerprint(request: &AgentEnrollmentRequest) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    request.node_name.hash(&mut hasher);
    request.hostname.hash(&mut hasher);
    request.os.hash(&mut hasher);
    request.architecture.hash(&mut hasher);
    request.agent_version.hash(&mut hasher);
    format!("dev-{:016x}", hasher.finish())
}

pub(crate) async fn agent_transport_websocket(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_agent_transport_socket(socket, state))
}

#[allow(clippy::too_many_lines)]
async fn handle_agent_transport_socket(mut socket: WebSocket, state: AppState) {
    let Some(Ok(first_message)) = socket.recv().await else {
        return;
    };
    let handshake = match parse_transport_envelope(first_message)
        .and_then(|envelope| negotiate_transport(&state, envelope))
    {
        Ok(handshake) => handshake,
        Err(error) => {
            let _ = send_transport_error(
                &mut socket,
                AgentTransportMessageId::first(),
                NodeId("unknown".to_owned()),
                AgentTransportId("unknown".to_owned()),
                transport_error_for_connection(error),
            )
            .await;
            return;
        }
    };

    let node_id_text = handshake.node_id.0.clone();
    let transport_id_text = handshake.transport_id.0.clone();
    let (command_tx, mut command_rx) = mpsc::channel(AGENT_TRANSPORT_CHANNEL_CAPACITY);
    let (event_tx, event_rx) = mpsc::channel(AGENT_TRANSPORT_CHANNEL_CAPACITY);
    let registration = state.agent_connections.register_transport(
        node_id_text.clone(),
        handshake.transport_id.clone(),
        handshake.selected_transport,
        command_tx,
        event_rx,
    );
    if registration == AgentConnectionRegistration::ReplacedExisting {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::AgentDisconnected,
            actor_email: None,
            message: format!(
                "duplicate agent transport replaced existing connection for node {node_id_text}"
            ),
        });
    }

    state.audit.record(AuditEventInput {
        kind: AuditEventKind::TransportNegotiated,
        actor_email: None,
        message: format!(
            "agent transport {transport_id_text} negotiated for node {node_id_text} using {:?}",
            handshake.selected_transport
        ),
    });
    state.audit.record(AuditEventInput {
        kind: AuditEventKind::AgentConnected,
        actor_email: None,
        message: format!("agent node {node_id_text} connected on transport {transport_id_text}"),
    });
    info!(
        node_id = %node_id_text,
        transport_id = %transport_id_text,
        "agent transport connected"
    );

    if send_transport_envelope(
        &mut socket,
        AgentTransportEnvelope::current(
            AgentTransportMessageId(2),
            handshake.node_id.clone(),
            handshake.transport_id.clone(),
            AgentTransportPayload::ServerHello {
                hello: handshake.server_hello(),
            },
        ),
    )
    .await
    .is_err()
    {
        state
            .agent_connections
            .disconnect_transport(&node_id_text, &handshake.transport_id);
        return;
    }

    let (mut sender, mut receiver) = socket.split();
    let mut next_message_id = AgentTransportMessageId(3);
    let mut last_heartbeat_at = Instant::now();
    let mut liveness_check = time::interval(AGENT_TRANSPORT_HEARTBEAT_INTERVAL);
    let mut disconnect_reason = "agent websocket closed".to_owned();

    loop {
        tokio::select! {
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(message)) => {
                        let frame_result = match parse_transport_envelope(message) {
                            Ok(envelope) => {
                                handle_agent_transport_envelope(&handshake, envelope, &event_tx)
                            }
                            Err(error) => Err(transport_error(
                                AgentTransportErrorCode::MalformedMessage,
                                format!("invalid transport frame: {error:?}"),
                            )),
                        };
                        match frame_result {
                            Ok(TransportFrameOutcome::HeartbeatReceived { ping_message_id }) => {
                                last_heartbeat_at = Instant::now();
                                let pong = AgentTransportEnvelope::current(
                                    next_message_id,
                                    handshake.node_id.clone(),
                                    handshake.transport_id.clone(),
                                    AgentTransportPayload::Heartbeat {
                                        heartbeat: AgentTransportHeartbeat::Pong {
                                            ping_message_id,
                                            received_at_unix_ms: 0,
                                        },
                                    },
                                );
                                next_message_id = next_message_id.next();
                                if send_split_transport_envelope(&mut sender, pong).await.is_err() {
                                    "failed to send heartbeat acknowledgement".clone_into(&mut disconnect_reason);
                                    break;
                                }
                            }
                            Ok(TransportFrameOutcome::TerminalEventReceived | TransportFrameOutcome::Ignored) => {}
                            Err(error) => {
                                warn!(
                                    node_id = %node_id_text,
                                    transport_id = %transport_id_text,
                                    code = ?error.code,
                                    message = %error.message,
                                    "agent transport frame rejected"
                                );
                                let error_envelope = AgentTransportEnvelope::current(
                                    next_message_id,
                                    handshake.node_id.clone(),
                                    handshake.transport_id.clone(),
                                    AgentTransportPayload::Error {
                                        error: transport_error(
                                            AgentTransportErrorCode::MalformedMessage,
                                            format!("invalid transport frame: {}", error.message),
                                        ),
                                    },
                                );
                                next_message_id = next_message_id.next();
                                let _ = send_split_transport_envelope(&mut sender, error_envelope).await;
                            }
                        }
                    }
                    Some(Err(error)) => {
                        disconnect_reason = format!("agent websocket error: {error}");
                        break;
                    }
                    None => break,
                }
            }
            command = command_rx.recv() => {
                let Some(command) = command else {
                    "agent command channel closed".clone_into(&mut disconnect_reason);
                    break;
                };
                let envelope = AgentTransportEnvelope::current(
                    next_message_id,
                    handshake.node_id.clone(),
                    handshake.transport_id.clone(),
                    AgentTransportPayload::TerminalCommand { command },
                );
                next_message_id = next_message_id.next();
                if send_split_transport_envelope(&mut sender, envelope).await.is_err() {
                    "failed to send terminal command to agent".clone_into(&mut disconnect_reason);
                    break;
                }
            }
            _ = liveness_check.tick() => {
                if last_heartbeat_at.elapsed() >= AGENT_TRANSPORT_LIVENESS_TIMEOUT {
                    "agent transport heartbeat timeout".clone_into(&mut disconnect_reason);
                    break;
                }
            }
        }
    }

    state
        .agent_connections
        .disconnect_transport(&node_id_text, &handshake.transport_id);
    state.audit.record(AuditEventInput {
        kind: AuditEventKind::AgentDisconnected,
        actor_email: None,
        message: format!(
            "agent node {node_id_text} disconnected from transport {transport_id_text}: {disconnect_reason}"
        ),
    });
    warn!(
        node_id = %node_id_text,
        transport_id = %transport_id_text,
        reason = %disconnect_reason,
        "agent transport disconnected"
    );
}

#[derive(Debug, Clone)]
struct AgentTransportHandshake {
    node_id: NodeId,
    transport_id: AgentTransportId,
    selected_transport: AgentTransportKind,
}

impl AgentTransportHandshake {
    fn server_hello(&self) -> AgentTransportServerHello {
        AgentTransportServerHello {
            accepted_protocol_version: PROTOCOL_VERSION,
            selected_transport: self.selected_transport,
            heartbeat_interval_ms: millis(AGENT_TRANSPORT_HEARTBEAT_INTERVAL),
            liveness_timeout_ms: millis(AGENT_TRANSPORT_LIVENESS_TIMEOUT),
            max_in_flight_messages: AGENT_TRANSPORT_MAX_IN_FLIGHT_MESSAGES,
            reconnect_policy: AgentTransportReconnectPolicy::production_default(),
            resume: AgentTransportResumeDecision::NotRequested,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum TransportFrameOutcome {
    HeartbeatReceived {
        ping_message_id: AgentTransportMessageId,
    },
    TerminalEventReceived,
    Ignored,
}

fn negotiate_transport(
    state: &AppState,
    envelope: AgentTransportEnvelope,
) -> Result<AgentTransportHandshake, AgentTransportConnectionError> {
    envelope
        .validate()
        .map_err(|_| AgentTransportConnectionError::InvalidEnvelope)?;
    let AgentTransportPayload::ClientHello { hello } = envelope.payload else {
        return Err(AgentTransportConnectionError::MissingClientHello);
    };
    validate_client_hello(&hello)?;
    authenticate_transport_client(state, &envelope.node_id, &hello)?;

    Ok(AgentTransportHandshake {
        node_id: envelope.node_id,
        transport_id: envelope.transport_id,
        selected_transport: AgentTransportKind::WebSocketTlsTcp443,
    })
}

fn validate_client_hello(
    hello: &AgentTransportClientHello,
) -> Result<(), AgentTransportConnectionError> {
    if !hello
        .supported_protocol_versions
        .contains(&PROTOCOL_VERSION)
    {
        return Err(AgentTransportConnectionError::UnsupportedProtocolVersion);
    }
    if !hello
        .supported_transports
        .contains(&AgentTransportKind::WebSocketTlsTcp443)
    {
        return Err(AgentTransportConnectionError::UnsupportedTransport);
    }

    Ok(())
}

fn authenticate_transport_client(
    state: &AppState,
    node_id: &NodeId,
    hello: &AgentTransportClientHello,
) -> Result<NodeView, AgentTransportConnectionError> {
    state
        .node_enrollment
        .authenticate_transport(
            &node_id.0,
            &hello.credential_fingerprint,
            &hello.agent_version,
        )
        .map_err(|error| match error {
            NodeConnectionError::UnknownNode | NodeConnectionError::InvalidCredential => {
                AgentTransportConnectionError::AuthenticationFailed
            }
            NodeConnectionError::Revoked => AgentTransportConnectionError::Revoked,
        })
}

fn handle_agent_transport_envelope(
    handshake: &AgentTransportHandshake,
    envelope: AgentTransportEnvelope,
    event_tx: &mpsc::Sender<AgentTerminalEvent>,
) -> Result<TransportFrameOutcome, AgentTransportError> {
    envelope.validate().map_err(|error| {
        transport_error(
            AgentTransportErrorCode::MalformedMessage,
            format!("invalid transport envelope: {error}"),
        )
    })?;
    if envelope.node_id != handshake.node_id || envelope.transport_id != handshake.transport_id {
        return Err(transport_error(
            AgentTransportErrorCode::MalformedMessage,
            "transport envelope identity does not match connection",
        ));
    }

    match envelope.payload {
        AgentTransportPayload::Heartbeat {
            heartbeat: AgentTransportHeartbeat::Ping { .. },
        } => Ok(TransportFrameOutcome::HeartbeatReceived {
            ping_message_id: envelope.message_id,
        }),
        AgentTransportPayload::TerminalEvent { event, .. } => {
            event_tx.try_send(event).map_err(|_| {
                transport_error(
                    AgentTransportErrorCode::BackpressureLimitExceeded,
                    "terminal event channel is full",
                )
            })?;
            Ok(TransportFrameOutcome::TerminalEventReceived)
        }
        AgentTransportPayload::Error { error } => {
            warn!(
                node_id = %handshake.node_id.0,
                transport_id = %handshake.transport_id.0,
                code = ?error.code,
                message = %error.message,
                "agent reported transport error"
            );
            Ok(TransportFrameOutcome::Ignored)
        }
        _ => Ok(TransportFrameOutcome::Ignored),
    }
}

fn parse_transport_envelope(
    message: Message,
) -> Result<AgentTransportEnvelope, AgentTransportConnectionError> {
    match message {
        Message::Text(text) => {
            serde_json::from_str(&text).map_err(|_| AgentTransportConnectionError::InvalidEnvelope)
        }
        Message::Binary(_) | Message::Close(_) | Message::Ping(_) | Message::Pong(_) => {
            Err(AgentTransportConnectionError::InvalidEnvelope)
        }
    }
}

async fn send_transport_envelope(
    socket: &mut WebSocket,
    envelope: AgentTransportEnvelope,
) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(
            serialize_transport_envelope(&envelope).into(),
        ))
        .await
}

async fn send_split_transport_envelope(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    envelope: AgentTransportEnvelope,
) -> Result<(), axum::Error> {
    sender
        .send(Message::Text(
            serialize_transport_envelope(&envelope).into(),
        ))
        .await
}

async fn send_transport_error(
    socket: &mut WebSocket,
    message_id: AgentTransportMessageId,
    node_id: NodeId,
    transport_id: AgentTransportId,
    error: AgentTransportError,
) -> Result<(), axum::Error> {
    send_transport_envelope(
        socket,
        AgentTransportEnvelope::current(
            message_id,
            node_id,
            transport_id,
            AgentTransportPayload::Error { error },
        ),
    )
    .await
}

fn serialize_transport_envelope(envelope: &AgentTransportEnvelope) -> String {
    serde_json::to_string(envelope).expect("transport envelopes should serialize")
}

fn transport_error(
    code: AgentTransportErrorCode,
    message: impl Into<String>,
) -> AgentTransportError {
    AgentTransportError {
        code,
        message: message.into(),
    }
}

fn transport_error_for_connection(error: AgentTransportConnectionError) -> AgentTransportError {
    match error {
        AgentTransportConnectionError::MissingClientHello
        | AgentTransportConnectionError::InvalidEnvelope => transport_error(
            AgentTransportErrorCode::MalformedMessage,
            "first agent transport frame must be a valid client hello",
        ),
        AgentTransportConnectionError::UnsupportedProtocolVersion => transport_error(
            AgentTransportErrorCode::UnsupportedProtocolVersion,
            "agent transport protocol version is not supported",
        ),
        AgentTransportConnectionError::UnsupportedTransport => transport_error(
            AgentTransportErrorCode::UnsupportedTransport,
            "agent transport must support websocket over TLS/TCP/443",
        ),
        AgentTransportConnectionError::AuthenticationFailed => transport_error(
            AgentTransportErrorCode::AuthenticationFailed,
            "agent transport authentication failed",
        ),
        AgentTransportConnectionError::Revoked => transport_error(
            AgentTransportErrorCode::NodeRevoked,
            "agent node is revoked",
        ),
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{
        handle_agent_transport_envelope, validate_client_hello, AgentConnectionRegistration,
        AgentConnectionRegistry, AgentTransportHandshake,
    };
    use sunbolt_protocol::{
        transport::{
            AgentTransportClientHello, AgentTransportEnvelope, AgentTransportHeartbeat,
            AgentTransportId, AgentTransportKind, AgentTransportMessageId, AgentTransportPayload,
            TerminalOutputSequence,
        },
        AgentTerminalCommand, AgentTerminalEvent, NodeId, TerminalSessionId, TerminalSize,
        PROTOCOL_VERSION,
    };
    use tokio::sync::mpsc;

    #[test]
    fn client_hello_requires_supported_protocol_and_websocket_transport() {
        let mut hello = valid_hello();
        assert!(validate_client_hello(&hello).is_ok());

        hello.supported_protocol_versions = vec![PROTOCOL_VERSION + 1];
        assert!(validate_client_hello(&hello).is_err());

        let mut hello = valid_hello();
        hello.supported_transports = vec![AgentTransportKind::QuicUdp443];
        assert!(validate_client_hello(&hello).is_err());
    }

    #[test]
    fn registry_replaces_duplicate_node_connection() {
        let registry = AgentConnectionRegistry::default();
        let (first_command_tx, _first_command_rx) = mpsc::channel(1);
        let (_first_event_tx, first_event_rx) = mpsc::channel(1);
        let (second_command_tx, _second_command_rx) = mpsc::channel(1);
        let (_second_event_tx, second_event_rx) = mpsc::channel(1);

        let first = registry.register_transport(
            "node-1",
            AgentTransportId("transport-1".to_owned()),
            AgentTransportKind::WebSocketTlsTcp443,
            first_command_tx,
            first_event_rx,
        );
        let second = registry.register_transport(
            "node-1",
            AgentTransportId("transport-2".to_owned()),
            AgentTransportKind::WebSocketTlsTcp443,
            second_command_tx,
            second_event_rx,
        );

        let connection = registry
            .connection("node-1")
            .expect("replacement connection should be active");

        assert_eq!(first, AgentConnectionRegistration::Registered);
        assert_eq!(second, AgentConnectionRegistration::ReplacedExisting);
        assert_eq!(
            connection.transport_id,
            AgentTransportId("transport-2".to_owned())
        );
        assert_eq!(registry.len(), 1);
        assert!(
            !registry.disconnect_transport("node-1", &AgentTransportId("transport-1".to_owned()))
        );
        assert_eq!(registry.len(), 1);
        assert!(
            registry.disconnect_transport("node-1", &AgentTransportId("transport-2".to_owned()))
        );
        assert_eq!(registry.len(), 0);
    }

    #[tokio::test]
    async fn transport_envelope_routes_terminal_event_to_registered_channel() {
        let handshake = AgentTransportHandshake {
            node_id: NodeId("node-1".to_owned()),
            transport_id: AgentTransportId("transport-1".to_owned()),
            selected_transport: AgentTransportKind::WebSocketTlsTcp443,
        };
        let (event_tx, mut event_rx) = mpsc::channel(1);
        let event = AgentTerminalEvent::TerminalOutput {
            session_id: TerminalSessionId("session-1".to_owned()),
            data: "hello\n".to_owned(),
        };
        let envelope = AgentTransportEnvelope::current(
            AgentTransportMessageId::first(),
            handshake.node_id.clone(),
            handshake.transport_id.clone(),
            AgentTransportPayload::TerminalEvent {
                sequence: Some(TerminalOutputSequence::first()),
                event: event.clone(),
            },
        );

        handle_agent_transport_envelope(&handshake, envelope, &event_tx)
            .expect("event should route");

        assert_eq!(event_rx.recv().await, Some(event));
    }

    #[test]
    fn transport_envelope_accepts_heartbeat_ping() {
        let handshake = AgentTransportHandshake {
            node_id: NodeId("node-1".to_owned()),
            transport_id: AgentTransportId("transport-1".to_owned()),
            selected_transport: AgentTransportKind::WebSocketTlsTcp443,
        };
        let (event_tx, _event_rx) = mpsc::channel(1);
        let envelope = AgentTransportEnvelope::current(
            AgentTransportMessageId(7),
            handshake.node_id.clone(),
            handshake.transport_id.clone(),
            AgentTransportPayload::Heartbeat {
                heartbeat: AgentTransportHeartbeat::Ping {
                    sent_at_unix_ms: 42,
                },
            },
        );

        let outcome = handle_agent_transport_envelope(&handshake, envelope, &event_tx)
            .expect("heartbeat should be accepted");

        assert!(matches!(
            outcome,
            super::TransportFrameOutcome::HeartbeatReceived {
                ping_message_id: AgentTransportMessageId(7),
            }
        ));
    }

    fn valid_hello() -> AgentTransportClientHello {
        AgentTransportClientHello {
            supported_protocol_versions: vec![PROTOCOL_VERSION],
            supported_transports: vec![
                AgentTransportKind::WebSocketTlsTcp443,
                AgentTransportKind::Http2TlsTcp443,
            ],
            preferred_transport: AgentTransportKind::WebSocketTlsTcp443,
            agent_version: "0.1.0".to_owned(),
            credential_fingerprint: "dev-fingerprint".to_owned(),
            resume: None,
        }
    }

    #[allow(dead_code)]
    fn _command_for_lint() -> AgentTerminalCommand {
        AgentTerminalCommand::StartTerminal {
            session_id: TerminalSessionId("session-1".to_owned()),
            size: TerminalSize { cols: 80, rows: 24 },
        }
    }
}
