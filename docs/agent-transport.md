# Agent Transport

Sunbolt owns the agent-control-plane transport. The core product must not depend on a third-party tunnel provider.

Agent nodes initiate outbound connections to the control plane. Production deployments must work when an agent node can only reach the internet through outbound ports `80` and `443`.

## Transport Priorities

1. Baseline production transport: TLS over TCP/443 using WebSocket or HTTP/2.
2. Optional fast path: QUIC over UDP/443 when the network allows it.
3. Restrictive-network fallback: long-poll or request/response control channel if required.

QUIC is useful for multiplexed streams, connection migration, and transport-level flow control, but it must not be the only production path because many enterprise networks block UDP.

## Transport Abstraction

Terminal and session logic should depend on a transport abstraction instead of a concrete WebSocket, HTTP/2, or QUIC implementation.

The abstraction should represent:

- Connection lifecycle state.
- Version negotiation.
- Authenticated node identity.
- Heartbeats and liveness checks.
- Backoff and reconnect.
- Resume after transient disconnect.
- Backpressure.
- Message IDs.
- Terminal output sequence numbers.
- Metrics and structured logs.

## Baseline TCP/443 Transport

The production baseline should run over TLS/TCP/443, using WebSocket or HTTP/2. The agent initiates the connection and keeps it alive with heartbeats.

The control plane should:

- Authenticate node identity during connection setup.
- Negotiate protocol and transport version.
- Detect stale connections by heartbeat timeout.
- Reject revoked nodes.
- Handle duplicate connections for the same node.
- Route terminal commands to the selected active transport.
- Emit audit events for transport negotiation, connection, and disconnect.

The agent should:

- Store durable node identity material safely on disk.
- Reconnect with backoff after transient failures.
- Resume or report non-recoverable terminal state after reconnect.
- Apply backpressure instead of unbounded buffering.

## Protocol Requirements

Transport messages should be versioned and serialized through `serde` types in `sunbolt-protocol`.

Each message that changes state or participates in terminal streaming should carry enough identity for tracing and recovery:

- Protocol version.
- Message ID.
- Node ID.
- Transport ID.
- Terminal session ID when applicable.
- Output sequence number for terminal output.

## Observability

Transport logs should use `tracing` fields such as:

- `node_id`
- `transport_id`
- `session_id`
- `request_id`

Transport metrics should cover connection state, heartbeat latency, reconnect count, bytes in/out, queued messages, dropped messages, and backpressure events.

## Current Development Path

The current local agent flow still supports enrollment and heartbeat HTTP endpoints for development iteration.

The baseline Sunbolt-native transport now has a WebSocket-over-TLS/TCP/443 control-plane route at `/agent/transport/ws`. After enrollment, an agent can derive an outbound `wss://` endpoint, send a versioned client hello with its node identity fingerprint, and use the negotiated channel for heartbeats and terminal command/event envelopes. The control plane applies a liveness timeout, replaces duplicate node transports, and records transport negotiation, agent connected, and agent disconnected audit events.
