use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    AgentTerminalCommand, AgentTerminalEvent, NodeId, TerminalSessionId, PROTOCOL_VERSION,
};

/// Stable identifier for one agent transport connection.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct AgentTransportId(pub String);

impl fmt::Display for AgentTransportId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Monotonic identifier for transport messages on one connection.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct AgentTransportMessageId(pub u64);

impl fmt::Display for AgentTransportMessageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl AgentTransportMessageId {
    /// Returns the first valid message identifier for a transport.
    #[must_use]
    pub const fn first() -> Self {
        Self(1)
    }

    /// Creates a message identifier when the value is non-zero.
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    /// Returns the numeric message identifier.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the next message identifier, saturating at `u64::MAX`.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    /// Returns true when this identifier is valid on the wire.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.0 != 0
    }
}

/// Monotonic sequence number for terminal output emitted by an agent stream.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct TerminalOutputSequence(pub u64);

impl fmt::Display for TerminalOutputSequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl TerminalOutputSequence {
    /// Returns the first output sequence for a terminal stream.
    #[must_use]
    pub const fn first() -> Self {
        Self(0)
    }

    /// Returns the numeric output sequence.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the next output sequence, saturating at `u64::MAX`.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Agent-control-plane transport implementation selected for a connection.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTransportKind {
    WebSocketTlsTcp443,
    Http2TlsTcp443,
    QuicUdp443,
    LongPollHttps,
}

impl AgentTransportKind {
    /// Returns true for transports that satisfy the production outbound TCP/443 baseline.
    #[must_use]
    pub const fn is_tcp443_baseline(self) -> bool {
        matches!(self, Self::WebSocketTlsTcp443 | Self::Http2TlsTcp443)
    }

    /// Returns true for the optional UDP/443 fast path.
    #[must_use]
    pub const fn is_udp443_fast_path(self) -> bool {
        matches!(self, Self::QuicUdp443)
    }

    /// Returns true when the transport is the restrictive-network HTTP fallback.
    #[must_use]
    pub const fn is_restrictive_network_fallback(self) -> bool {
        matches!(self, Self::LongPollHttps)
    }
}

/// QUIC implementation selected for the Sunbolt fast-path adapter.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AgentQuicImplementation {
    Quinn,
}

/// Logical stream roles used by the QUIC fast path.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AgentQuicStreamKind {
    Control,
    TerminalInput,
    TerminalOutput,
    TerminalResize,
    TerminalLifecycle,
}

/// Mapping from protocol messages to QUIC stream behavior.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AgentQuicStreamMapping {
    pub kind: AgentQuicStreamKind,
    pub bidirectional: bool,
    pub ordered: bool,
    pub carries_terminal_output_sequence: bool,
}

/// Design contract for the optional QUIC fast path.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AgentQuicFastPathPlan {
    pub implementation: AgentQuicImplementation,
    pub udp_port: u16,
    pub alpn: &'static str,
    pub fallback_order: &'static [AgentTransportKind],
    pub stream_mapping: &'static [AgentQuicStreamMapping],
}

impl AgentQuicFastPathPlan {
    /// Returns the current QUIC design contract.
    #[must_use]
    pub const fn current() -> Self {
        Self {
            implementation: AgentQuicImplementation::Quinn,
            udp_port: 443,
            alpn: "sunbolt-agent/1",
            fallback_order: &QUIC_FALLBACK_ORDER,
            stream_mapping: &QUIC_STREAM_MAPPING,
        }
    }
}

const QUIC_FALLBACK_ORDER: [AgentTransportKind; 3] = [
    AgentTransportKind::WebSocketTlsTcp443,
    AgentTransportKind::Http2TlsTcp443,
    AgentTransportKind::LongPollHttps,
];

const QUIC_STREAM_MAPPING: [AgentQuicStreamMapping; 5] = [
    AgentQuicStreamMapping {
        kind: AgentQuicStreamKind::Control,
        bidirectional: true,
        ordered: true,
        carries_terminal_output_sequence: false,
    },
    AgentQuicStreamMapping {
        kind: AgentQuicStreamKind::TerminalInput,
        bidirectional: false,
        ordered: true,
        carries_terminal_output_sequence: false,
    },
    AgentQuicStreamMapping {
        kind: AgentQuicStreamKind::TerminalOutput,
        bidirectional: false,
        ordered: true,
        carries_terminal_output_sequence: true,
    },
    AgentQuicStreamMapping {
        kind: AgentQuicStreamKind::TerminalResize,
        bidirectional: false,
        ordered: true,
        carries_terminal_output_sequence: false,
    },
    AgentQuicStreamMapping {
        kind: AgentQuicStreamKind::TerminalLifecycle,
        bidirectional: true,
        ordered: true,
        carries_terminal_output_sequence: false,
    },
];

/// Lifecycle states for an outbound agent transport connection.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTransportLifecycleState {
    Created,
    Connecting,
    Negotiating,
    Authenticating,
    Connected,
    Degraded,
    Reconnecting,
    Draining,
    Disconnected,
    Failed,
    Closed,
}

impl AgentTransportLifecycleState {
    /// Returns true when terminal commands can still be routed through the connection.
    #[must_use]
    pub const fn can_route_terminal_streams(self) -> bool {
        matches!(self, Self::Connected | Self::Degraded)
    }

    /// Returns true when reconnect policy may create a replacement connection.
    #[must_use]
    pub const fn can_reconnect(self) -> bool {
        matches!(self, Self::Disconnected | Self::Failed | Self::Reconnecting)
    }
}

/// Transport metrics fields that concrete implementations must maintain.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentTransportMetrics {
    pub lifecycle_state: AgentTransportLifecycleState,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub heartbeats_sent: u64,
    pub heartbeats_received: u64,
    pub reconnect_attempts: u64,
    pub queued_messages: u64,
    pub dropped_messages: u64,
    pub backpressure_events: u64,
    pub last_heartbeat_rtt_ms: Option<u64>,
}

impl AgentTransportMetrics {
    /// Creates empty metrics for a new transport state.
    #[must_use]
    pub const fn new(lifecycle_state: AgentTransportLifecycleState) -> Self {
        Self {
            lifecycle_state,
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            heartbeats_sent: 0,
            heartbeats_received: 0,
            reconnect_attempts: 0,
            queued_messages: 0,
            dropped_messages: 0,
            backpressure_events: 0,
            last_heartbeat_rtt_ms: None,
        }
    }
}

/// Backoff and resume policy for transient transport disconnects.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentTransportReconnectPolicy {
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub multiplier: u8,
    pub jitter_percent: u8,
    pub max_attempts: Option<u32>,
    pub resume_window_ms: u64,
}

impl AgentTransportReconnectPolicy {
    /// Returns a conservative production default for reconnect backoff.
    #[must_use]
    pub const fn production_default() -> Self {
        Self {
            initial_backoff_ms: 1_000,
            max_backoff_ms: 30_000,
            multiplier: 2,
            jitter_percent: 20,
            max_attempts: None,
            resume_window_ms: 120_000,
        }
    }

    /// Returns the bounded delay before the next reconnect attempt.
    #[must_use]
    pub fn delay_for_attempt(self, attempt: u32) -> u64 {
        let mut delay = self.initial_backoff_ms.max(1);
        for _ in 0..attempt {
            delay = delay.saturating_mul(u64::from(self.multiplier.max(1)));
            if delay >= self.max_backoff_ms {
                return self.max_backoff_ms;
            }
        }
        delay.min(self.max_backoff_ms)
    }
}

/// Resume cursor sent during negotiation after a transient disconnect.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentTransportResumeRequest {
    pub previous_transport_id: AgentTransportId,
    pub last_received_message_id: AgentTransportMessageId,
    pub terminal_sequences: Vec<TerminalStreamResumeCursor>,
}

/// Last terminal output sequence observed for one terminal session.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TerminalStreamResumeCursor {
    pub session_id: TerminalSessionId,
    pub last_output_sequence: TerminalOutputSequence,
}

/// Result of a reconnect resume negotiation.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTransportResumeDecision {
    NotRequested,
    Accepted,
    Rejected { reason: String },
}

/// Agent-to-control-plane negotiation message.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentTransportClientHello {
    pub supported_protocol_versions: Vec<u16>,
    pub supported_transports: Vec<AgentTransportKind>,
    pub preferred_transport: AgentTransportKind,
    pub agent_version: String,
    pub credential_fingerprint: String,
    pub credential_proof: String,
    pub resume: Option<AgentTransportResumeRequest>,
}

/// Control-plane-to-agent negotiation response.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentTransportServerHello {
    pub accepted_protocol_version: u16,
    pub selected_transport: AgentTransportKind,
    pub heartbeat_interval_ms: u64,
    pub liveness_timeout_ms: u64,
    pub max_in_flight_messages: u32,
    pub reconnect_policy: AgentTransportReconnectPolicy,
    pub resume: AgentTransportResumeDecision,
}

/// Agent long-poll request for restrictive networks that only permit
/// request/response HTTPS traffic.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentTransportLongPollRequest {
    pub client_hello: AgentTransportEnvelope,
    pub events: Vec<AgentTransportEnvelope>,
}

/// Control-plane response to one restrictive-network long-poll request.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentTransportLongPollResponse {
    pub envelopes: Vec<AgentTransportEnvelope>,
    pub retry_after_ms: u64,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
}

/// Transport heartbeat messages exchanged after negotiation succeeds.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentTransportHeartbeat {
    Ping {
        sent_at_unix_ms: u64,
    },
    Pong {
        ping_message_id: AgentTransportMessageId,
        received_at_unix_ms: u64,
    },
}

/// Error code safe to report across the transport.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTransportErrorCode {
    UnsupportedProtocolVersion,
    UnsupportedTransport,
    AuthenticationFailed,
    NodeRevoked,
    DuplicateConnection,
    HeartbeatTimeout,
    BackpressureLimitExceeded,
    MissingMessageId,
    MissingTerminalOutputSequence,
    UnexpectedTerminalOutputSequence,
    InvalidTerminalSequenceOrder,
    MalformedMessage,
    Internal,
}

/// Structured transport error sent between agent and control plane.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentTransportError {
    pub code: AgentTransportErrorCode,
    pub message: String,
}

/// Payload carried by an agent transport envelope.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentTransportPayload {
    ClientHello {
        hello: AgentTransportClientHello,
    },
    ServerHello {
        hello: AgentTransportServerHello,
    },
    Heartbeat {
        heartbeat: AgentTransportHeartbeat,
    },
    TerminalCommand {
        command: AgentTerminalCommand,
    },
    TerminalEvent {
        sequence: Option<TerminalOutputSequence>,
        event: AgentTerminalEvent,
    },
    Error {
        error: AgentTransportError,
    },
}

/// Versioned transport envelope used by all concrete agent transports.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentTransportEnvelope {
    pub protocol_version: u16,
    pub message_id: AgentTransportMessageId,
    pub node_id: NodeId,
    pub transport_id: AgentTransportId,
    pub payload: AgentTransportPayload,
}

impl AgentTransportEnvelope {
    /// Creates an envelope for the current Sunbolt protocol version.
    #[must_use]
    pub fn current(
        message_id: AgentTransportMessageId,
        node_id: NodeId,
        transport_id: AgentTransportId,
        payload: AgentTransportPayload,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            message_id,
            node_id,
            transport_id,
            payload,
        }
    }

    /// Validates protocol-version, message-ID, and terminal sequence requirements.
    ///
    /// # Errors
    ///
    /// Returns an error when the envelope violates the shared transport contract.
    pub fn validate(&self) -> Result<(), AgentTransportProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(AgentTransportProtocolError::UnsupportedProtocolVersion {
                received: self.protocol_version,
                supported: PROTOCOL_VERSION,
            });
        }

        if !self.message_id.is_valid() {
            return Err(AgentTransportProtocolError::MissingMessageId);
        }

        match &self.payload {
            AgentTransportPayload::TerminalEvent { sequence, event } => match event {
                AgentTerminalEvent::TerminalOutput { .. } if sequence.is_none() => {
                    Err(AgentTransportProtocolError::MissingTerminalOutputSequence)
                }
                AgentTerminalEvent::TerminalOutput { .. } => Ok(()),
                _ if sequence.is_some() => {
                    Err(AgentTransportProtocolError::UnexpectedTerminalOutputSequence)
                }
                _ => Ok(()),
            },
            _ => Ok(()),
        }
    }
}

/// Transport validation errors detected before dispatching a frame.
#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum AgentTransportProtocolError {
    #[error("unsupported protocol version {received}; supported version is {supported}")]
    UnsupportedProtocolVersion { received: u16, supported: u16 },
    #[error("transport message id must be non-zero")]
    MissingMessageId,
    #[error("terminal output events must include a sequence number")]
    MissingTerminalOutputSequence,
    #[error("terminal sequence numbers are only valid for terminal output events")]
    UnexpectedTerminalOutputSequence,
    #[error("terminal output sequence {received} did not follow {previous}")]
    InvalidTerminalSequenceOrder {
        previous: TerminalOutputSequence,
        received: TerminalOutputSequence,
    },
}

/// I/O-agnostic boundary implemented by concrete agent transport adapters.
pub trait AgentTransport {
    /// Returns the managed node associated with the transport.
    fn node_id(&self) -> &NodeId;

    /// Returns the unique transport connection identifier.
    fn transport_id(&self) -> &AgentTransportId;

    /// Returns the current lifecycle state.
    fn lifecycle_state(&self) -> AgentTransportLifecycleState;

    /// Returns a snapshot of transport metrics.
    fn metrics(&self) -> AgentTransportMetrics;

    /// Returns the reconnect policy selected during negotiation.
    fn reconnect_policy(&self) -> AgentTransportReconnectPolicy;

    /// Queues a validated transport envelope for delivery.
    ///
    /// # Errors
    ///
    /// Returns an error when the frame violates transport protocol requirements
    /// or the implementation cannot accept more messages.
    fn send(&mut self, envelope: AgentTransportEnvelope) -> Result<(), AgentTransportError>;

    /// Starts an orderly transport shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error when the implementation cannot send or record the close.
    fn close(&mut self, reason: AgentTransportCloseReason) -> Result<(), AgentTransportError>;
}

/// Reason a transport connection closed.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTransportCloseReason {
    AgentShutdown,
    ControlPlaneShutdown,
    HeartbeatTimeout,
    DuplicateConnectionReplaced,
    NodeRevoked,
    ProtocolError,
    NetworkError,
}

/// Validates that a terminal output sequence follows the previous value.
///
/// # Errors
///
/// Returns an error when `received` is not exactly `previous.next()`.
pub fn validate_next_terminal_output_sequence(
    previous: TerminalOutputSequence,
    received: TerminalOutputSequence,
) -> Result<(), AgentTransportProtocolError> {
    if received == previous.next() {
        Ok(())
    } else {
        Err(AgentTransportProtocolError::InvalidTerminalSequenceOrder { previous, received })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        validate_next_terminal_output_sequence, AgentQuicFastPathPlan, AgentQuicImplementation,
        AgentQuicStreamKind, AgentTransportClientHello, AgentTransportEnvelope,
        AgentTransportError, AgentTransportErrorCode, AgentTransportHeartbeat, AgentTransportId,
        AgentTransportKind, AgentTransportLifecycleState, AgentTransportMessageId,
        AgentTransportMetrics, AgentTransportPayload, AgentTransportProtocolError,
        AgentTransportReconnectPolicy, AgentTransportResumeDecision, AgentTransportServerHello,
        TerminalOutputSequence,
    };
    use crate::{
        AgentTerminalEvent, NodeId, TerminalError, TerminalErrorCode, TerminalSessionId,
        PROTOCOL_VERSION,
    };

    #[test]
    fn transport_message_ids_are_non_zero_and_monotonic() {
        let first = AgentTransportMessageId::first();

        assert_eq!(AgentTransportMessageId::new(0), None);
        assert_eq!(
            AgentTransportMessageId::new(7),
            Some(AgentTransportMessageId(7))
        );
        assert_eq!(first.get(), 1);
        assert_eq!(first.next(), AgentTransportMessageId(2));
    }

    #[test]
    fn terminal_output_sequences_start_at_zero_and_increment() {
        let first = TerminalOutputSequence::first();

        assert_eq!(first.get(), 0);
        assert_eq!(first.next(), TerminalOutputSequence(1));
        assert!(validate_next_terminal_output_sequence(first, first.next()).is_ok());
        assert_eq!(
            validate_next_terminal_output_sequence(first, TerminalOutputSequence(3)),
            Err(AgentTransportProtocolError::InvalidTerminalSequenceOrder {
                previous: first,
                received: TerminalOutputSequence(3),
            })
        );
    }

    #[test]
    fn lifecycle_state_identifies_routable_and_reconnectable_states() {
        assert!(AgentTransportLifecycleState::Connected.can_route_terminal_streams());
        assert!(AgentTransportLifecycleState::Degraded.can_route_terminal_streams());
        assert!(!AgentTransportLifecycleState::Negotiating.can_route_terminal_streams());
        assert!(AgentTransportLifecycleState::Disconnected.can_reconnect());
        assert!(!AgentTransportLifecycleState::Closed.can_reconnect());
    }

    #[test]
    fn metrics_start_empty_for_state() {
        let metrics = AgentTransportMetrics::new(AgentTransportLifecycleState::Created);

        assert_eq!(
            metrics.lifecycle_state,
            AgentTransportLifecycleState::Created
        );
        assert_eq!(metrics.messages_sent, 0);
        assert_eq!(metrics.backpressure_events, 0);
        assert_eq!(metrics.last_heartbeat_rtt_ms, None);
    }

    #[test]
    fn reconnect_policy_is_bounded() {
        let policy = AgentTransportReconnectPolicy::production_default();

        assert_eq!(policy.delay_for_attempt(0), 1_000);
        assert_eq!(policy.delay_for_attempt(1), 2_000);
        assert_eq!(policy.delay_for_attempt(8), 30_000);
        assert_eq!(policy.resume_window_ms, 120_000);
    }

    #[test]
    fn tcp443_baseline_transports_are_explicit() {
        assert!(AgentTransportKind::WebSocketTlsTcp443.is_tcp443_baseline());
        assert!(AgentTransportKind::Http2TlsTcp443.is_tcp443_baseline());
        assert!(!AgentTransportKind::QuicUdp443.is_tcp443_baseline());
        assert!(!AgentTransportKind::LongPollHttps.is_tcp443_baseline());
        assert!(AgentTransportKind::QuicUdp443.is_udp443_fast_path());
        assert!(!AgentTransportKind::WebSocketTlsTcp443.is_udp443_fast_path());
        assert!(AgentTransportKind::LongPollHttps.is_restrictive_network_fallback());
    }

    #[test]
    fn quic_fast_path_plan_preserves_tcp_and_long_poll_fallbacks() {
        let plan = AgentQuicFastPathPlan::current();

        assert_eq!(plan.implementation, AgentQuicImplementation::Quinn);
        assert_eq!(plan.udp_port, 443);
        assert_eq!(plan.alpn, "sunbolt-agent/1");
        assert_eq!(
            plan.fallback_order,
            &[
                AgentTransportKind::WebSocketTlsTcp443,
                AgentTransportKind::Http2TlsTcp443,
                AgentTransportKind::LongPollHttps,
            ]
        );
        assert!(plan.stream_mapping.iter().any(|mapping| {
            mapping.kind == AgentQuicStreamKind::TerminalOutput
                && mapping.ordered
                && mapping.carries_terminal_output_sequence
        }));
        assert!(plan.stream_mapping.iter().any(|mapping| {
            mapping.kind == AgentQuicStreamKind::Control && mapping.bidirectional
        }));
    }

    #[test]
    fn serializes_negotiation_envelope() {
        let envelope = AgentTransportEnvelope::current(
            AgentTransportMessageId::first(),
            NodeId("node-1".to_owned()),
            AgentTransportId("transport-1".to_owned()),
            AgentTransportPayload::ClientHello {
                hello: AgentTransportClientHello {
                    supported_protocol_versions: vec![PROTOCOL_VERSION],
                    supported_transports: vec![
                        AgentTransportKind::WebSocketTlsTcp443,
                        AgentTransportKind::Http2TlsTcp443,
                    ],
                    preferred_transport: AgentTransportKind::WebSocketTlsTcp443,
                    agent_version: "0.1.0".to_owned(),
                    credential_fingerprint: "dev-fingerprint".to_owned(),
                    credential_proof: "dev-proof".to_owned(),
                    resume: None,
                },
            },
        );

        let value = serde_json::to_value(envelope).expect("envelope should serialize");

        assert_eq!(
            value,
            json!({
                "protocol_version": 1,
                "message_id": 1,
                "node_id": "node-1",
                "transport_id": "transport-1",
                "payload": {
                    "type": "client_hello",
                    "hello": {
                        "supported_protocol_versions": [1],
                        "supported_transports": ["web_socket_tls_tcp443", "http2_tls_tcp443"],
                        "preferred_transport": "web_socket_tls_tcp443",
                        "agent_version": "0.1.0",
                        "credential_fingerprint": "dev-fingerprint",
                        "credential_proof": "dev-proof",
                        "resume": null
                    }
                }
            })
        );
    }

    #[test]
    fn deserializes_server_hello_and_heartbeat() {
        let hello: AgentTransportPayload = serde_json::from_value(json!({
            "type": "server_hello",
            "hello": {
                "accepted_protocol_version": 1,
                "selected_transport": "web_socket_tls_tcp443",
                "heartbeat_interval_ms": 30000,
                "liveness_timeout_ms": 90000,
                "max_in_flight_messages": 256,
                "reconnect_policy": {
                    "initial_backoff_ms": 1000,
                    "max_backoff_ms": 30000,
                    "multiplier": 2,
                    "jitter_percent": 20,
                    "max_attempts": null,
                    "resume_window_ms": 120_000
                },
                "resume": "not_requested"
            }
        }))
        .expect("server hello should deserialize");

        assert_eq!(
            hello,
            AgentTransportPayload::ServerHello {
                hello: AgentTransportServerHello {
                    accepted_protocol_version: PROTOCOL_VERSION,
                    selected_transport: AgentTransportKind::WebSocketTlsTcp443,
                    heartbeat_interval_ms: 30_000,
                    liveness_timeout_ms: 90_000,
                    max_in_flight_messages: 256,
                    reconnect_policy: AgentTransportReconnectPolicy::production_default(),
                    resume: AgentTransportResumeDecision::NotRequested,
                },
            }
        );

        let heartbeat: AgentTransportHeartbeat = serde_json::from_value(json!({
            "kind": "pong",
            "ping_message_id": 12,
            "received_at_unix_ms": 42
        }))
        .expect("heartbeat should deserialize");

        assert_eq!(
            heartbeat,
            AgentTransportHeartbeat::Pong {
                ping_message_id: AgentTransportMessageId(12),
                received_at_unix_ms: 42,
            }
        );
    }

    #[test]
    fn envelope_validation_rejects_invalid_protocol_and_message_ids() {
        let mut envelope = terminal_error_envelope(None);
        envelope.protocol_version = PROTOCOL_VERSION + 1;

        assert_eq!(
            envelope.validate(),
            Err(AgentTransportProtocolError::UnsupportedProtocolVersion {
                received: PROTOCOL_VERSION + 1,
                supported: PROTOCOL_VERSION,
            })
        );

        let mut envelope = terminal_error_envelope(None);
        envelope.message_id = AgentTransportMessageId(0);

        assert_eq!(
            envelope.validate(),
            Err(AgentTransportProtocolError::MissingMessageId)
        );
    }

    #[test]
    fn terminal_output_events_require_sequence_numbers() {
        let output = AgentTerminalEvent::TerminalOutput {
            session_id: TerminalSessionId("session-1".to_owned()),
            data: "hello\n".to_owned(),
        };
        let missing_sequence = terminal_event_envelope(None, output.clone());

        assert_eq!(
            missing_sequence.validate(),
            Err(AgentTransportProtocolError::MissingTerminalOutputSequence)
        );

        let sequenced = terminal_event_envelope(Some(TerminalOutputSequence::first()), output);

        assert!(sequenced.validate().is_ok());
    }

    #[test]
    fn non_output_terminal_events_reject_sequence_numbers() {
        let event = AgentTerminalEvent::TerminalError {
            session_id: TerminalSessionId("session-1".to_owned()),
            error: TerminalError {
                code: TerminalErrorCode::TerminalUnavailable,
                message: "terminal failed".to_owned(),
            },
        };
        let envelope = terminal_event_envelope(Some(TerminalOutputSequence::first()), event);

        assert_eq!(
            envelope.validate(),
            Err(AgentTransportProtocolError::UnexpectedTerminalOutputSequence)
        );
    }

    #[test]
    fn transport_error_payload_is_structured() {
        let envelope = AgentTransportEnvelope::current(
            AgentTransportMessageId::first(),
            NodeId("node-1".to_owned()),
            AgentTransportId("transport-1".to_owned()),
            AgentTransportPayload::Error {
                error: AgentTransportError {
                    code: AgentTransportErrorCode::DuplicateConnection,
                    message: "duplicate node transport replaced".to_owned(),
                },
            },
        );

        assert!(envelope.validate().is_ok());
    }

    fn terminal_error_envelope(sequence: Option<TerminalOutputSequence>) -> AgentTransportEnvelope {
        terminal_event_envelope(
            sequence,
            AgentTerminalEvent::TerminalError {
                session_id: TerminalSessionId("session-1".to_owned()),
                error: TerminalError {
                    code: TerminalErrorCode::TerminalUnavailable,
                    message: "terminal failed".to_owned(),
                },
            },
        )
    }

    fn terminal_event_envelope(
        sequence: Option<TerminalOutputSequence>,
        event: AgentTerminalEvent,
    ) -> AgentTransportEnvelope {
        AgentTransportEnvelope::current(
            AgentTransportMessageId::first(),
            NodeId("node-1".to_owned()),
            AgentTransportId("transport-1".to_owned()),
            AgentTransportPayload::TerminalEvent { sequence, event },
        )
    }
}
