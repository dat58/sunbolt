# Sunbolt

Sunbolt is a Rust-first remote terminal and distributed server control platform.

The MVP is complete. The current direction is production hardening: Sunbolt should safely manage browser terminal sessions across many servers through a centralized control plane and outbound-only managed agents.

Target production flow:

```text
Browser -> Control Plane -> Sunbolt Native Outbound Agent Channel -> Agent Node -> PTY
```

The control plane is the source of truth for identity, authentication, authorization, MFA policy, node enrollment and revocation, terminal session metadata, route policy, and audit logs.

## Production Direction

Sunbolt is designed around these production constraints:

- Agent nodes initiate outbound connections to the control plane and do not require inbound firewall access.
- The baseline production agent transport must work over TLS/TCP/443, using WebSocket or HTTP/2.
- QUIC over UDP/443 may become an optional fast path, but it must not be the only production transport.
- Terminal sessions must survive browser navigation, refreshes, tab switches, and short WebSocket disconnects by detaching and reattaching instead of killing the PTY.
- Production state belongs in PostgreSQL. In-memory maps may hold live sockets, PTY handles, and short-lived runtime handles, but not durable source-of-truth state.
- Production terminal operations require server-side authentication, authorization, optional step-up MFA, audit records, and lifecycle tracking.
- The UI must work across mobile, tablet, laptop, and desktop devices.

## Runtime Modes

Sunbolt has exactly two runtime modes:

- `development`
- `production`

Development mode may use local bootstrap credentials, permissive local browser origins, and temporary in-memory scaffolding while features are under construction.

Production mode must use explicit configuration, secure cookies, durable storage, non-wildcard browser origins, and no development bootstrap admin accounts.

Preview and test deployments should use one of these two modes with environment-specific configuration, secrets, domains, and infrastructure.

## Workspace

The Rust workspace separates core areas of responsibility:

- `sunbolt-ui`: Dioxus web UI shell, pages, components, and browser integration.
- `sunbolt-control`: Axum control plane, HTTP and WebSocket routes, policy orchestration, and agent coordination.
- `sunbolt-agent`: managed node daemon and outbound control-plane channel.
- `sunbolt-common`: shared identifiers and small cross-cutting helpers.
- `sunbolt-auth`: authentication, MFA, sessions, and RBAC policy primitives.
- `sunbolt-terminal`: PTY and terminal lifecycle primitives.
- `sunbolt-protocol`: versioned browser/control/agent protocol messages.
- `sunbolt-audit`: audit event types, append-only writing, chain verification, and export.
- `sunbolt-storage`: PostgreSQL connection and repository boundaries.

## Documentation

- [Production plan](PLAN.md)
- [Production task list](TASK.md)
- [Local development](docs/local-development.md)
- [Deployment](docs/deployment.md)
- [Security model](docs/security-model.md)
- [Terminal lifecycle](docs/terminal-lifecycle.md)
- [Agent transport](docs/agent-transport.md)
- [UI architecture](docs/ui-architecture.md)
- [Audit events](docs/audit-events.md)

## Local Checks

Run these before committing code changes:

```bash
cargo fmt
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
