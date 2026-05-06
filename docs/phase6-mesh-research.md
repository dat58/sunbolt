# Phase 6 Mesh Research

Sunbolt should keep the control plane as the source of truth while it explores
mesh-style routing. The near-term target is not pure peer-to-peer terminal
access. It is centrally authorized routing where the control plane selects and
audits paths, and agents execute only scoped route grants.

## Transport Evaluation

QUIC is the best prototype candidate for future agent streams. It gives
Sunbolt multiplexed streams, transport security, connection migration, and
flow-control primitives without requiring operators to configure host-level
network overlays.

WireGuard remains a useful deployment option for operators that already manage
private overlays. It should be deferred as a required Sunbolt feature because it
adds host networking assumptions and operational setup outside the application.

## Node-to-Node Relay Mode

Relay mode should be control-plane authorized. A relay node may forward traffic
for a target node only after receiving a short-lived route grant scoped to:

- actor
- terminal session
- target node
- relay node
- expiration

The relay should not become a second source of identity, authorization, or
audit state.

## Trust Model

Node enrollment starts with a one-time enrollment token. Long-term trust should
move to node identity material that can be rotated and revoked by the control
plane.

Nodes do not trust peer nodes by default. Peer communication requires a
control-plane-issued route grant, and future relay payloads should be encrypted
so a relay only forwards bytes unless explicitly authorized for inspection.

## Audit Implications

Relay routing needs audit events beyond terminal open and close:

- route selected
- relay grant issued
- relay started
- relay ended
- relay failed

Each relay audit record should include actor, terminal session, target node,
relay node, route kind, grant id, and failure reason when available.
