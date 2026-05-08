# Audit Events

Audit logs answer who did what, to which resource, when, and whether it was allowed. Operational logs answer how components interacted and why operations succeeded, retried, degraded, or failed.

Audit events are security records and should be append-only from the application perspective.

## Event Shape

Audit events should carry:

- Event ID.
- Event kind.
- Timestamp.
- Actor ID or actor email when available.
- Workspace ID when applicable.
- Node ID when applicable.
- Terminal session ID when applicable.
- Transport ID when applicable.
- Route ID when applicable.
- Outcome such as allowed, denied, succeeded, failed, or degraded.
- Reason or error code when applicable.
- Request ID for correlation.

Do not include secrets in audit events.

## Security Audit Events

Security-relevant audit events should include:

- `user.login.succeeded`
- `user.login.failed`
- `mfa.challenge.created`
- `mfa.challenge.succeeded`
- `mfa.challenge.failed`
- `terminal.opened`
- `terminal.detached`
- `terminal.reattached`
- `terminal.terminated`
- `terminal.failed`
- `node.enrolled`
- `node.revoked`
- `node.credential.rotated`
- `permission.changed`

## Agent and Transport Events

Agent and transport audit events should include:

- `agent.connected`
- `agent.disconnected`
- `agent.authentication.failed`
- `agent.transport.negotiated`
- `route.selected`
- `route.failed`

Some agent and route events are both operationally useful and security-relevant. When an event affects trust, access, routing, or terminal availability, it should have an audit record as well as structured logs.

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
