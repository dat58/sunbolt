# Phase 6 Control Plane HA

Sunbolt should support multiple control-plane instances without moving identity,
authorization, or audit decisions out of the control plane. HA should start with
shared state and active-session routing, not full multi-region routing.

## Shared State

The following state must be shared or made recoverable before running multiple
active control-plane instances:

- users
- sessions
- MFA challenges and recent MFA timestamps
- RBAC policy
- nodes
- node credentials
- node heartbeats
- terminal session metadata
- route health
- audit logs

Live terminal byte streams can remain instance-local during the first HA phase,
but their metadata and ownership leases must be shared.

## Routing Backend Evaluation

Postgres notification channels are the baseline because the project already
targets PostgreSQL. They are appropriate for coarse invalidation and ownership
notifications, but not for high-rate terminal streams.

Redis is the best first prototype for active-session routing. Expiring keys,
leases, and low-latency pub/sub fit agent presence, route health, and terminal
session ownership.

NATS should be evaluated later for larger distributed deployments. It is a good
fit for a message-bus architecture, but it adds operational complexity before
Sunbolt needs that scale.

## Agent Connection Strategy

Each agent should have one active control-plane owner at a time. Ownership is a
lease backed by shared routing state. If the owning instance stops acknowledging
heartbeats or pings, the agent reconnects with backoff and any healthy instance
may validate identity and claim the lease.

Node identity and credential validation must remain backed by shared storage.
An instance-local agent map is only a live transport cache.

## Sticky WebSocket Routing

Active browser terminal WebSockets should be sticky to the control-plane
instance that owns the browser socket and the selected agent route. Reconnect
and reattach flows should consult shared session metadata to locate the owner or
return a clear expired-session error when ownership is gone.
