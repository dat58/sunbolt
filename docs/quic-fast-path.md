# QUIC Fast Path Design Spike

Sunbolt's production baseline remains TLS over outbound TCP/443. QUIC over UDP/443 is an optional fast path for networks that allow outbound UDP and for deployments that can expose UDP/443 at the public edge.

## Implementation Choice

Use `quinn` for the Rust QUIC adapter.

Reasons:

- It is a mature Rust QUIC implementation built on Rustls.
- It exposes bidirectional and unidirectional streams directly, which fits Sunbolt terminal control and data flow.
- It supports transport-level flow control and connection migration without requiring a kernel overlay or third-party tunnel.
- It is already present in the dependency graph through existing HTTP client dependencies, so adding a first-party adapter later should not introduce a new protocol family into the build.

Do not make QUIC the only production transport. Enterprise egress filters often block UDP even when TCP/443 is allowed.

## Endpoint and Negotiation

The agent derives a candidate endpoint from the control-plane URL:

```text
quic://<control-plane-host>/agent/transport/quic
```

The QUIC adapter should use UDP/443 and ALPN:

```text
sunbolt-agent/1
```

The first application frame on the QUIC control stream is the same versioned `AgentTransportEnvelope` client hello used by the WebSocket and long-poll transports. The control plane authenticates node identity, validates protocol version, and selects `QuicUdp443` only when the QUIC adapter received the connection.

## Fallback Behavior

The agent attempt order is:

1. `QuicUdp443`
2. `WebSocketTlsTcp443`
3. `Http2TlsTcp443`
4. `LongPollHttps`

If UDP/443 is blocked, times out, or receives a policy rejection, the agent falls back to the TCP/443 baseline. If streaming TCP paths are blocked by a proxy, the agent falls back to long-poll HTTPS and reports degraded transport status to browser terminal sessions.

## Stream Mapping

The QUIC fast path maps Sunbolt messages onto ordered streams:

- Control stream: bidirectional stream for negotiation, heartbeat, errors, lifecycle events, and small control messages.
- Terminal input stream: unidirectional agent-bound stream for input bytes.
- Terminal output stream: unidirectional control-plane-bound stream for output bytes. Every terminal output event still carries a `TerminalOutputSequence`.
- Terminal resize stream: unidirectional agent-bound stream for resize messages.
- Terminal lifecycle stream: bidirectional stream for start, close, terminate, detach, reattach, and terminal errors.

Keep message IDs on every envelope even when QUIC stream ordering is available. Message IDs support audit correlation, resume cursors, duplicate detection, and cross-transport consistency.

## Current Status

The transport abstraction is stable enough to define the QUIC contract, advertise QUIC from the agent, and validate QUIC negotiation for a future adapter. The live production path remains WebSocket over TLS/TCP/443 with long-poll HTTPS fallback until a UDP listener and Quinn adapter are added behind the same `AgentTransport` boundary.
