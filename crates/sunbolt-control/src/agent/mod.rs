use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sunbolt_audit::{AuditEventInput, AuditEventKind};
use sunbolt_protocol::{
    transport::{
        AgentTransportClientHello, AgentTransportEnvelope, AgentTransportError,
        AgentTransportErrorCode, AgentTransportHeartbeat, AgentTransportId, AgentTransportKind,
        AgentTransportLongPollRequest, AgentTransportLongPollResponse, AgentTransportMessageId,
        AgentTransportPayload, AgentTransportReconnectPolicy, AgentTransportResumeDecision,
        AgentTransportServerHello,
    },
    AgentTerminalCommand, AgentTerminalEvent, NodeId, TerminalTransportStatus, PROTOCOL_VERSION,
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
const AGENT_TRANSPORT_LONG_POLL_WAIT: Duration = Duration::from_secs(25);
const AGENT_TRANSPORT_LONG_POLL_RETRY: Duration = Duration::from_secs(1);
const AGENT_TRANSPORT_LONG_POLL_BATCH: usize = 32;
const LONG_POLL_DEGRADED_REASON: &str =
    "restrictive network fallback active; terminal latency may be higher";
pub(crate) const NODE_CREDENTIAL_TTL: Duration = Duration::from_secs(90 * 24 * 60 * 60);

#[derive(Debug, Clone)]
pub(crate) struct RegisteredAgentConnection {
    pub(crate) command_tx: mpsc::Sender<AgentTerminalCommand>,
    pub(crate) event_rx: Arc<AsyncMutex<mpsc::Receiver<AgentTerminalEvent>>>,
    pub(crate) transport_id: AgentTransportId,
    pub(crate) transport_kind: AgentTransportKind,
    pub(crate) degraded_reason: Option<String>,
    long_poll: Option<LongPollConnectionState>,
}

#[derive(Debug, Clone)]
struct LongPollConnectionState {
    command_rx: Arc<AsyncMutex<mpsc::Receiver<AgentTerminalCommand>>>,
    event_tx: mpsc::Sender<AgentTerminalEvent>,
    next_message_id: Arc<Mutex<AgentTransportMessageId>>,
}

impl RegisteredAgentConnection {
    pub(crate) fn terminal_transport_status(&self) -> TerminalTransportStatus {
        TerminalTransportStatus {
            kind: self.transport_kind,
            degraded: self.transport_kind.is_restrictive_network_fallback(),
            message: self.degraded_reason.clone(),
        }
    }
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
                    transport_kind: AgentTransportKind::WebSocketTlsTcp443,
                    degraded_reason: None,
                    long_poll: None,
                },
            );
    }

    pub(crate) fn register_transport(
        &self,
        node_id: impl Into<String>,
        transport_id: AgentTransportId,
        transport_kind: AgentTransportKind,
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
                    transport_kind,
                    degraded_reason: None,
                    long_poll: None,
                },
            )
            .is_some();

        if replaced_existing {
            AgentConnectionRegistration::ReplacedExisting
        } else {
            AgentConnectionRegistration::Registered
        }
    }

    pub(crate) fn ensure_long_poll_transport(
        &self,
        node_id: impl Into<String>,
        transport_id: AgentTransportId,
    ) -> (AgentConnectionRegistration, RegisteredAgentConnection) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let node_id = node_id.into();
        if let Some(connection) = inner.get(&node_id) {
            if connection.transport_id == transport_id
                && connection.transport_kind == AgentTransportKind::LongPollHttps
            {
                return (AgentConnectionRegistration::Existing, connection.clone());
            }
        }

        let (command_tx, command_rx) = mpsc::channel(AGENT_TRANSPORT_CHANNEL_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(AGENT_TRANSPORT_CHANNEL_CAPACITY);
        let connection = RegisteredAgentConnection {
            command_tx,
            event_rx: Arc::new(AsyncMutex::new(event_rx)),
            transport_id,
            transport_kind: AgentTransportKind::LongPollHttps,
            degraded_reason: Some(LONG_POLL_DEGRADED_REASON.to_owned()),
            long_poll: Some(LongPollConnectionState {
                command_rx: Arc::new(AsyncMutex::new(command_rx)),
                event_tx,
                next_message_id: Arc::new(Mutex::new(AgentTransportMessageId(2))),
            }),
        };
        let replaced_existing = inner.insert(node_id, connection.clone()).is_some();
        let registration = if replaced_existing {
            AgentConnectionRegistration::ReplacedExisting
        } else {
            AgentConnectionRegistration::Registered
        };
        (registration, connection)
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
    Existing,
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
    pub(crate) credential_secret: String,
    pub(crate) credential_expires_at_unix_secs: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AgentHeartbeatRequest {
    pub(crate) node_id: String,
    pub(crate) credential_fingerprint: String,
    pub(crate) credential_proof: String,
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

pub(crate) fn generate_node_credential() -> (String, String) {
    let secret = crate::security::random_token();
    let fingerprint = credential_fingerprint(&secret);
    (secret, fingerprint)
}

pub(crate) fn credential_proof(node_id: &str, secret: &str) -> String {
    credential_fingerprint(&format!("sunbolt-agent-auth-v1\0{node_id}\0{secret}"))
}

pub(crate) fn credential_fingerprint(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    let mut fingerprint = String::with_capacity("sha256:".len() + digest.len() * 2);
    fingerprint.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(fingerprint, "{byte:02x}");
    }
    fingerprint
}

pub(crate) fn credential_expiration_unix_secs(now: SystemTime) -> i64 {
    let expires_at = now + NODE_CREDENTIAL_TTL;
    i64::try_from(
        expires_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
    .unwrap_or(i64::MAX)
}

pub(crate) async fn agent_transport_websocket(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_agent_transport_socket(socket, state))
}

pub(crate) async fn agent_transport_long_poll(
    State(state): State<AppState>,
    Json(request): Json<AgentTransportLongPollRequest>,
) -> impl IntoResponse {
    let failed_node_id = request.client_hello.node_id.clone();
    let failed_transport_id = request.client_hello.transport_id.clone();
    match handle_agent_transport_long_poll_request(&state, request).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => {
            record_agent_transport_authentication_failure(
                &state,
                &failed_node_id,
                &failed_transport_id,
                error,
            );
            (
                StatusCode::BAD_REQUEST,
                Json(AgentTransportLongPollResponse {
                    envelopes: vec![AgentTransportEnvelope::current(
                        AgentTransportMessageId::first(),
                        failed_node_id,
                        failed_transport_id,
                        AgentTransportPayload::Error {
                            error: transport_error_for_connection(error),
                        },
                    )],
                    retry_after_ms: millis(AGENT_TRANSPORT_LONG_POLL_RETRY),
                    degraded: true,
                    degraded_reason: Some(LONG_POLL_DEGRADED_REASON.to_owned()),
                }),
            )
                .into_response()
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_agent_transport_socket(mut socket: WebSocket, state: AppState) {
    let Some(Ok(first_message)) = socket.recv().await else {
        return;
    };
    let first_envelope = match parse_transport_envelope(first_message) {
        Ok(envelope) => envelope,
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
    let failed_node_id = first_envelope.node_id.clone();
    let failed_transport_id = first_envelope.transport_id.clone();
    let handshake = match negotiate_transport(&state, first_envelope) {
        Ok(handshake) => handshake,
        Err(error) => {
            record_agent_transport_authentication_failure(
                &state,
                &failed_node_id,
                &failed_transport_id,
                error,
            );
            let _ = send_transport_error(
                &mut socket,
                AgentTransportMessageId::first(),
                failed_node_id,
                failed_transport_id,
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
                let envelope = transport_command_envelope(&handshake, next_message_id, command);
                next_message_id = next_message_id.next();
                if send_split_transport_envelope(&mut sender, envelope).await.is_err() {
                    "failed to send terminal command to agent".clone_into(&mut disconnect_reason);
                    break;
                }
            }
            _ = liveness_check.tick() => {
                if heartbeat_timed_out(last_heartbeat_at, AGENT_TRANSPORT_LIVENESS_TIMEOUT) {
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
    negotiate_transport_for_kind(state, envelope, AgentTransportKind::WebSocketTlsTcp443)
}

fn negotiate_transport_for_kind(
    state: &AppState,
    envelope: AgentTransportEnvelope,
    selected_transport: AgentTransportKind,
) -> Result<AgentTransportHandshake, AgentTransportConnectionError> {
    envelope
        .validate()
        .map_err(|_| AgentTransportConnectionError::InvalidEnvelope)?;
    let AgentTransportPayload::ClientHello { hello } = envelope.payload else {
        return Err(AgentTransportConnectionError::MissingClientHello);
    };
    validate_client_hello_for_transport(&hello, selected_transport)?;
    authenticate_transport_client(state, &envelope.node_id, &hello)?;

    Ok(AgentTransportHandshake {
        node_id: envelope.node_id,
        transport_id: envelope.transport_id,
        selected_transport,
    })
}

#[cfg(test)]
fn validate_client_hello(
    hello: &AgentTransportClientHello,
) -> Result<(), AgentTransportConnectionError> {
    validate_client_hello_for_transport(hello, AgentTransportKind::WebSocketTlsTcp443)
}

#[cfg(test)]
fn validate_quic_client_hello(
    hello: &AgentTransportClientHello,
) -> Result<(), AgentTransportConnectionError> {
    validate_client_hello_for_transport(hello, AgentTransportKind::QuicUdp443)
}

fn validate_client_hello_for_transport(
    hello: &AgentTransportClientHello,
    required_transport: AgentTransportKind,
) -> Result<(), AgentTransportConnectionError> {
    if !hello
        .supported_protocol_versions
        .contains(&PROTOCOL_VERSION)
    {
        return Err(AgentTransportConnectionError::UnsupportedProtocolVersion);
    }
    if !hello.supported_transports.contains(&required_transport) {
        return Err(AgentTransportConnectionError::UnsupportedTransport);
    }

    Ok(())
}

async fn handle_agent_transport_long_poll_request(
    state: &AppState,
    request: AgentTransportLongPollRequest,
) -> Result<AgentTransportLongPollResponse, AgentTransportConnectionError> {
    let handshake = negotiate_transport_for_kind(
        state,
        request.client_hello,
        AgentTransportKind::LongPollHttps,
    )?;
    let node_id_text = handshake.node_id.0.clone();
    let (registration, connection) = state
        .agent_connections
        .ensure_long_poll_transport(node_id_text.clone(), handshake.transport_id.clone());

    record_long_poll_registration(&state.audit, registration, &handshake);

    let Some(long_poll) = connection.long_poll.clone() else {
        return Err(AgentTransportConnectionError::UnsupportedTransport);
    };
    let mut envelopes = vec![AgentTransportEnvelope::current(
        next_long_poll_message_id(&long_poll),
        handshake.node_id.clone(),
        handshake.transport_id.clone(),
        AgentTransportPayload::ServerHello {
            hello: handshake.server_hello(),
        },
    )];

    for envelope in request.events {
        match handle_agent_transport_envelope(&handshake, envelope, &long_poll.event_tx) {
            Ok(TransportFrameOutcome::HeartbeatReceived { ping_message_id }) => {
                envelopes.push(AgentTransportEnvelope::current(
                    next_long_poll_message_id(&long_poll),
                    handshake.node_id.clone(),
                    handshake.transport_id.clone(),
                    AgentTransportPayload::Heartbeat {
                        heartbeat: AgentTransportHeartbeat::Pong {
                            ping_message_id,
                            received_at_unix_ms: 0,
                        },
                    },
                ));
            }
            Ok(TransportFrameOutcome::TerminalEventReceived | TransportFrameOutcome::Ignored) => {}
            Err(error) => envelopes.push(AgentTransportEnvelope::current(
                next_long_poll_message_id(&long_poll),
                handshake.node_id.clone(),
                handshake.transport_id.clone(),
                AgentTransportPayload::Error { error },
            )),
        }
    }

    append_long_poll_commands(&mut envelopes, &handshake, &long_poll).await;

    Ok(AgentTransportLongPollResponse {
        envelopes,
        retry_after_ms: millis(AGENT_TRANSPORT_LONG_POLL_RETRY),
        degraded: true,
        degraded_reason: Some(LONG_POLL_DEGRADED_REASON.to_owned()),
    })
}

fn record_long_poll_registration(
    audit: &sunbolt_audit::AuditLog,
    registration: AgentConnectionRegistration,
    handshake: &AgentTransportHandshake,
) {
    let node_id_text = &handshake.node_id.0;
    let transport_id_text = &handshake.transport_id.0;
    match registration {
        AgentConnectionRegistration::Existing => {}
        AgentConnectionRegistration::Registered | AgentConnectionRegistration::ReplacedExisting => {
            if registration == AgentConnectionRegistration::ReplacedExisting {
                audit.record(AuditEventInput {
                    kind: AuditEventKind::AgentDisconnected,
                    actor_email: None,
                    message: format!(
                        "duplicate agent transport replaced existing connection for node {node_id_text}"
                    ),
                });
            }
            audit.record(AuditEventInput {
                kind: AuditEventKind::TransportNegotiated,
                actor_email: None,
                message: format!(
                    "agent transport {transport_id_text} negotiated for node {node_id_text} using {:?}",
                    handshake.selected_transport
                ),
            });
            audit.record(AuditEventInput {
                kind: AuditEventKind::AgentConnected,
                actor_email: None,
                message: format!(
                    "agent node {node_id_text} connected on degraded long-poll transport {transport_id_text}"
                ),
            });
            info!(
                node_id = %node_id_text,
                transport_id = %transport_id_text,
                "agent long-poll transport connected"
            );
        }
    }
}

async fn append_long_poll_commands(
    envelopes: &mut Vec<AgentTransportEnvelope>,
    handshake: &AgentTransportHandshake,
    long_poll: &LongPollConnectionState,
) {
    let mut command_rx = long_poll.command_rx.lock().await;
    if envelopes.len() == 1 {
        if let Ok(Some(command)) =
            time::timeout(AGENT_TRANSPORT_LONG_POLL_WAIT, command_rx.recv()).await
        {
            envelopes.push(command_envelope(handshake, long_poll, command));
        }
    }

    while envelopes.len() < AGENT_TRANSPORT_LONG_POLL_BATCH {
        match command_rx.try_recv() {
            Ok(command) => envelopes.push(command_envelope(handshake, long_poll, command)),
            Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected) => {
                break;
            }
        }
    }
}

fn command_envelope(
    handshake: &AgentTransportHandshake,
    long_poll: &LongPollConnectionState,
    command: AgentTerminalCommand,
) -> AgentTransportEnvelope {
    transport_command_envelope(handshake, next_long_poll_message_id(long_poll), command)
}

fn transport_command_envelope(
    handshake: &AgentTransportHandshake,
    message_id: AgentTransportMessageId,
    command: AgentTerminalCommand,
) -> AgentTransportEnvelope {
    AgentTransportEnvelope::current(
        message_id,
        handshake.node_id.clone(),
        handshake.transport_id.clone(),
        AgentTransportPayload::TerminalCommand { command },
    )
}

fn next_long_poll_message_id(long_poll: &LongPollConnectionState) -> AgentTransportMessageId {
    let mut next = long_poll
        .next_message_id
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let current = *next;
    *next = next.next();
    current
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
            &hello.credential_proof,
            &hello.agent_version,
        )
        .map_err(|error| match error {
            NodeConnectionError::UnknownNode
            | NodeConnectionError::InvalidCredential
            | NodeConnectionError::CredentialExpired => {
                AgentTransportConnectionError::AuthenticationFailed
            }
            NodeConnectionError::Revoked => AgentTransportConnectionError::Revoked,
        })
}

fn record_agent_transport_authentication_failure(
    state: &AppState,
    node_id: &NodeId,
    transport_id: &AgentTransportId,
    error: AgentTransportConnectionError,
) {
    if !matches!(
        error,
        AgentTransportConnectionError::AuthenticationFailed
            | AgentTransportConnectionError::Revoked
    ) {
        return;
    }

    state.audit.record(AuditEventInput {
        kind: AuditEventKind::AgentAuthenticationFailed,
        actor_email: None,
        message: format!(
            "agent transport authentication failed for node {} on transport {}: {}",
            node_id.0,
            transport_id.0,
            agent_transport_authentication_failure_reason(error)
        ),
    });
}

const fn agent_transport_authentication_failure_reason(
    error: AgentTransportConnectionError,
) -> &'static str {
    match error {
        AgentTransportConnectionError::AuthenticationFailed => "invalid node identity",
        AgentTransportConnectionError::Revoked => "node revoked",
        AgentTransportConnectionError::MissingClientHello
        | AgentTransportConnectionError::InvalidEnvelope
        | AgentTransportConnectionError::UnsupportedProtocolVersion
        | AgentTransportConnectionError::UnsupportedTransport => "not an authentication failure",
    }
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
            "agent transport does not support the required transport",
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

fn heartbeat_timed_out(last_heartbeat_at: Instant, timeout: Duration) -> bool {
    last_heartbeat_at.elapsed() >= timeout
}

#[cfg(test)]
mod tests {
    use super::{
        credential_proof, handle_agent_transport_envelope, heartbeat_timed_out,
        negotiate_transport, record_agent_transport_authentication_failure,
        transport_command_envelope, validate_client_hello, validate_client_hello_for_transport,
        validate_quic_client_hello, AgentConnectionRegistration, AgentConnectionRegistry,
        AgentEnrollmentRequest, AgentTransportHandshake,
    };
    use crate::{error::AgentTransportConnectionError, state::AppState};
    use std::time::Duration;
    use sunbolt_audit::AuditEventKind;
    use sunbolt_auth::{AuthConfig, AuthService, User, UserRole};
    use sunbolt_protocol::{
        transport::{
            AgentTransportClientHello, AgentTransportEnvelope, AgentTransportHeartbeat,
            AgentTransportId, AgentTransportKind, AgentTransportMessageId, AgentTransportPayload,
            AgentTransportReconnectPolicy, TerminalOutputSequence,
        },
        AgentTerminalCommand, AgentTerminalEvent, NodeId, TerminalSessionId, TerminalSize,
        PROTOCOL_VERSION,
    };
    use tokio::{sync::mpsc, time::Instant};

    #[test]
    fn client_hello_requires_supported_protocol_and_websocket_transport() {
        let mut hello = valid_hello();
        assert!(validate_client_hello(&hello).is_ok());
        assert!(validate_quic_client_hello(&hello).is_ok());

        hello.supported_protocol_versions = vec![PROTOCOL_VERSION + 1];
        assert!(validate_client_hello(&hello).is_err());

        let mut hello = valid_hello();
        hello.supported_transports = vec![AgentTransportKind::QuicUdp443];
        assert!(validate_client_hello(&hello).is_err());
        assert!(validate_quic_client_hello(&hello).is_ok());

        let mut hello = valid_hello();
        hello.supported_transports = vec![AgentTransportKind::LongPollHttps];
        assert!(
            validate_client_hello_for_transport(&hello, AgentTransportKind::LongPollHttps).is_ok()
        );
    }

    #[test]
    fn transport_negotiation_authenticates_node_and_selects_websocket_baseline() {
        let (state, node_id, credential_fingerprint, credential_secret) =
            state_with_enrolled_node();
        let envelope = client_hello_envelope(
            &node_id,
            "transport-1",
            valid_hello_for_credential(
                &credential_fingerprint,
                credential_proof(&node_id, &credential_secret),
            ),
        );

        let handshake = negotiate_transport(&state, envelope).expect("negotiation should succeed");
        let server_hello = handshake.server_hello();

        assert_eq!(handshake.node_id, NodeId(node_id));
        assert_eq!(
            handshake.transport_id,
            AgentTransportId("transport-1".to_owned())
        );
        assert_eq!(
            handshake.selected_transport,
            AgentTransportKind::WebSocketTlsTcp443
        );
        assert_eq!(
            server_hello.selected_transport,
            AgentTransportKind::WebSocketTlsTcp443
        );
        assert_eq!(
            server_hello.reconnect_policy,
            AgentTransportReconnectPolicy::production_default()
        );
    }

    #[test]
    fn transport_negotiation_rejects_unknown_node_identity() {
        let (_state_with_node, _node_id, credential_fingerprint, credential_secret) =
            state_with_enrolled_node();
        let state = AppState::development_with_auth(AuthService::new(AuthConfig {
            session_ttl: Duration::from_secs(60 * 60),
            recent_mfa_ttl: Duration::from_secs(10 * 60),
            secure_cookie: false,
            require_step_up_mfa_for_terminal: true,
            bootstrap_admin: false,
            admin_email: "unused@example.com".to_owned(),
            admin_password: "unused".to_owned(),
        }));
        let envelope = client_hello_envelope(
            "node-missing",
            "transport-1",
            valid_hello_for_credential(
                &credential_fingerprint,
                credential_proof("node-missing", &credential_secret),
            ),
        );

        let error = negotiate_transport(&state, envelope)
            .expect_err("unknown node identity should be rejected");

        assert_eq!(error, AgentTransportConnectionError::AuthenticationFailed);
    }

    #[test]
    fn transport_negotiation_rejects_invalid_credential_proof() {
        let (state, node_id, credential_fingerprint, _credential_secret) =
            state_with_enrolled_node();
        let envelope = client_hello_envelope(
            &node_id,
            "transport-1",
            valid_hello_for_credential(&credential_fingerprint, "wrong-proof"),
        );

        let error = negotiate_transport(&state, envelope)
            .expect_err("invalid credential proof should be rejected");

        assert_eq!(error, AgentTransportConnectionError::AuthenticationFailed);
    }

    #[test]
    fn transport_negotiation_rejects_expired_credential() {
        let (state, node_id, credential_fingerprint, credential_secret) =
            state_with_enrolled_node();
        assert!(state.node_enrollment.expire_credentials_for_node(&node_id));
        let envelope = client_hello_envelope(
            &node_id,
            "transport-1",
            valid_hello_for_credential(
                &credential_fingerprint,
                credential_proof(&node_id, &credential_secret),
            ),
        );

        let error = negotiate_transport(&state, envelope)
            .expect_err("expired credential should be rejected");

        assert_eq!(error, AgentTransportConnectionError::AuthenticationFailed);
    }

    #[test]
    fn transport_negotiation_rejects_revoked_node() {
        let (state, node_id, credential_fingerprint, credential_secret) =
            state_with_enrolled_node();
        state
            .node_enrollment
            .revoke_node(&node_id)
            .expect("node should revoke");
        let envelope = client_hello_envelope(
            &node_id,
            "transport-1",
            valid_hello_for_credential(
                &credential_fingerprint,
                credential_proof(&node_id, &credential_secret),
            ),
        );

        let error =
            negotiate_transport(&state, envelope).expect_err("revoked node should be rejected");

        assert_eq!(error, AgentTransportConnectionError::Revoked);
    }

    #[test]
    fn failed_transport_authentication_is_audited() {
        let (state, node_id, credential_fingerprint, _credential_secret) =
            state_with_enrolled_node();
        let envelope = client_hello_envelope(
            &node_id,
            "transport-1",
            valid_hello_for_credential(&credential_fingerprint, "wrong-proof"),
        );
        let failed_node_id = envelope.node_id.clone();
        let failed_transport_id = envelope.transport_id.clone();
        let error = negotiate_transport(&state, envelope)
            .expect_err("invalid credential proof should be rejected");

        record_agent_transport_authentication_failure(
            &state,
            &failed_node_id,
            &failed_transport_id,
            error,
        );

        assert!(state.audit.events().iter().any(|event| {
            event.kind == AuditEventKind::AgentAuthenticationFailed
                && event.message.contains("invalid node identity")
        }));
    }

    #[test]
    fn heartbeat_timeout_detects_stale_connections_without_waiting() {
        let stale = Instant::now() - Duration::from_secs(91);
        let fresh = Instant::now() - Duration::from_secs(30);

        assert!(heartbeat_timed_out(stale, Duration::from_secs(90)));
        assert!(!heartbeat_timed_out(fresh, Duration::from_secs(90)));
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

    #[test]
    fn registry_marks_long_poll_transport_as_degraded() {
        let registry = AgentConnectionRegistry::default();

        let (first, connection) = registry
            .ensure_long_poll_transport("node-1", AgentTransportId("transport-1".to_owned()));
        let (second, same_connection) = registry
            .ensure_long_poll_transport("node-1", AgentTransportId("transport-1".to_owned()));

        assert_eq!(first, AgentConnectionRegistration::Registered);
        assert_eq!(second, AgentConnectionRegistration::Existing);
        assert_eq!(connection.transport_kind, AgentTransportKind::LongPollHttps);
        assert!(connection.terminal_transport_status().degraded);
        assert!(same_connection.long_poll.is_some());
        assert_eq!(registry.len(), 1);
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
    fn baseline_transport_command_routing_wraps_terminal_commands() {
        let handshake = AgentTransportHandshake {
            node_id: NodeId("node-1".to_owned()),
            transport_id: AgentTransportId("transport-1".to_owned()),
            selected_transport: AgentTransportKind::WebSocketTlsTcp443,
        };
        let command = AgentTerminalCommand::StartTerminal {
            session_id: TerminalSessionId("session-1".to_owned()),
            size: TerminalSize {
                cols: 100,
                rows: 30,
            },
        };

        let envelope =
            transport_command_envelope(&handshake, AgentTransportMessageId(9), command.clone());

        assert_eq!(envelope.message_id, AgentTransportMessageId(9));
        assert_eq!(envelope.node_id, handshake.node_id);
        assert_eq!(envelope.transport_id, handshake.transport_id);
        assert_eq!(
            envelope.payload,
            AgentTransportPayload::TerminalCommand { command }
        );
        assert!(envelope.validate().is_ok());
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

    #[test]
    fn transport_backpressure_rejects_full_terminal_event_channel() {
        let handshake = AgentTransportHandshake {
            node_id: NodeId("node-1".to_owned()),
            transport_id: AgentTransportId("transport-1".to_owned()),
            selected_transport: AgentTransportKind::WebSocketTlsTcp443,
        };
        let (event_tx, _event_rx) = mpsc::channel(1);
        let first = terminal_output_envelope(&handshake, AgentTransportMessageId(1));
        let second = terminal_output_envelope(&handshake, AgentTransportMessageId(2));

        handle_agent_transport_envelope(&handshake, first, &event_tx)
            .expect("first event should fit in the bounded channel");
        let error = handle_agent_transport_envelope(&handshake, second, &event_tx)
            .expect_err("second event should hit the backpressure limit");

        assert_eq!(
            error.code,
            super::AgentTransportErrorCode::BackpressureLimitExceeded
        );
    }

    fn valid_hello() -> AgentTransportClientHello {
        valid_hello_for_credential("dev-fingerprint", "dev-proof")
    }

    fn valid_hello_for_credential(
        credential_fingerprint: impl Into<String>,
        credential_proof: impl Into<String>,
    ) -> AgentTransportClientHello {
        AgentTransportClientHello {
            supported_protocol_versions: vec![PROTOCOL_VERSION],
            supported_transports: vec![
                AgentTransportKind::QuicUdp443,
                AgentTransportKind::WebSocketTlsTcp443,
                AgentTransportKind::Http2TlsTcp443,
            ],
            preferred_transport: AgentTransportKind::QuicUdp443,
            agent_version: "0.1.0".to_owned(),
            credential_fingerprint: credential_fingerprint.into(),
            credential_proof: credential_proof.into(),
            resume: None,
        }
    }

    fn terminal_output_envelope(
        handshake: &AgentTransportHandshake,
        message_id: AgentTransportMessageId,
    ) -> AgentTransportEnvelope {
        AgentTransportEnvelope::current(
            message_id,
            handshake.node_id.clone(),
            handshake.transport_id.clone(),
            AgentTransportPayload::TerminalEvent {
                sequence: Some(TerminalOutputSequence::first()),
                event: AgentTerminalEvent::TerminalOutput {
                    session_id: TerminalSessionId("session-1".to_owned()),
                    data: "hello\n".to_owned(),
                },
            },
        )
    }

    fn client_hello_envelope(
        node_id: &str,
        transport_id: &str,
        hello: AgentTransportClientHello,
    ) -> AgentTransportEnvelope {
        AgentTransportEnvelope::current(
            AgentTransportMessageId::first(),
            NodeId(node_id.to_owned()),
            AgentTransportId(transport_id.to_owned()),
            AgentTransportPayload::ClientHello { hello },
        )
    }

    fn state_with_enrolled_node() -> (AppState, String, String, String) {
        let state = AppState::development_with_auth(AuthService::new(AuthConfig {
            session_ttl: Duration::from_secs(60 * 60),
            recent_mfa_ttl: Duration::from_secs(10 * 60),
            secure_cookie: false,
            require_step_up_mfa_for_terminal: true,
            bootstrap_admin: false,
            admin_email: "unused@example.com".to_owned(),
            admin_password: "unused".to_owned(),
        }));
        let token = state.node_enrollment.create_token(
            &User {
                id: 1,
                email: "admin@example.com".to_owned(),
                role: UserRole::Admin,
            },
            Duration::from_secs(300),
        );
        let enrollment = state
            .node_enrollment
            .enroll(AgentEnrollmentRequest {
                token: token.token,
                node_name: "node-a".to_owned(),
                hostname: "host-a".to_owned(),
                os: "linux".to_owned(),
                architecture: "x86_64".to_owned(),
                agent_version: "0.1.0".to_owned(),
            })
            .expect("node should enroll");

        (
            state,
            enrollment.node_id,
            enrollment.credential_fingerprint,
            enrollment.credential_secret,
        )
    }

    #[allow(dead_code)]
    fn _command_for_lint() -> AgentTerminalCommand {
        AgentTerminalCommand::StartTerminal {
            session_id: TerminalSessionId("session-1".to_owned()),
            size: TerminalSize { cols: 80, rows: 24 },
        }
    }
}
