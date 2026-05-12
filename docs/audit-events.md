# Audit Events

Audit logs answer who did what, to which resource, when, and whether it was allowed. Operational logs answer how components interacted and why operations succeeded, retried, degraded, or failed.

Audit events are security records and should be append-only from the application perspective.

## Event Schema

The current in-process audit record is serialized with these fields:

- `id`: append-only event sequence ID.
- `kind`: stable dotted event name, such as `terminal.opened`.
- `actor_email`: actor email when available.
- `message`: redacted human-readable event summary.
- `created_at_unix_secs`: event timestamp.
- `previous_hash`: previous audit event hash.
- `event_hash`: hash of this event and the previous hash.

The durable audit repository boundary is designed to carry richer production
metadata as storage matures:

- `user_id` when the actor is a persisted user.
- `event_type`, matching the stable dotted event name.
- `target`, such as a terminal session, node, transport, or route.
- `metadata_json`, for redacted structured context such as request ID, outcome, reason, workspace ID, node ID, session ID, transport ID, and route ID.
- `ip_address` when captured at the public edge.
- `created_at_unix_secs`.

Do not include secrets in audit events.

## Security Audit Events

Security audit events are append-only records for access, trust, identity, and terminal lifecycle decisions. They currently include:

- `user.login.success`
- `user.login.failed`
- `user.logout`
- `user.mfa.challenge`
- `user.mfa.success`
- `terminal.opened`
- `terminal.detached`
- `terminal.reattached`
- `terminal.terminated`
- `terminal.closed`
- `terminal.failed`
- `agent.authentication.failed`
- `node.enrolled`
- `node.revoked`
- `node.credential.rotated`

Planned security audit events include:

- `permission.changed`

## Operational Audit Events

Operational audit events describe control-plane, agent, transport, and route decisions that operators need for diagnosis and incident reconstruction. They should also have structured `tracing` logs with the same identifiers.

Current operational audit event names are:

- `agent.connected`
- `agent.disconnected`
- `agent.transport.negotiated`
- `route.selected`
- `route.failed`

Agent authentication failures are security audit events because they affect node trust. Agent connect, disconnect, transport negotiation, and route decisions are operational audit events because they primarily explain availability and routing behavior.

## Structured Logging Fields

Operational logs should use `tracing` spans and fields consistently:

- `request_id`
- `actor_id`
- `actor_email`
- `workspace_id`
- `node_id`
- `session_id`
- `transport_id`
- `route_id`

Logs should describe retries, disconnects, degraded transport behavior, backpressure, policy decisions, and recoverable failures.

## Redaction

Never log or audit raw secrets:

- Passwords.
- Cookies.
- Enrollment tokens.
- Node credentials.
- Recovery codes.
- Passkey credential material.
- Long-lived opaque bearer identifiers.

If an identifier is needed for correlation, store a stable database ID or a short fingerprint that cannot be used as a credential.
