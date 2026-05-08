# PLAN.md

# Sunbolt Production Plan

Sunbolt is a Rust-first remote terminal and distributed server control platform.

The MVP is complete. The next product phase is production hardening: make Sunbolt secure, reliable, observable, responsive across device classes, and ready to operate real agent-managed server terminals.

All project documentation, code comments, identifiers, commit messages, and user-facing product text must be written in English.

## Product Direction

Sunbolt should let authorized users open and manage secure terminal sessions to controlled servers from modern browsers on:

- Mobile phones
- Tablets
- Laptops
- Desktops

The system must work in restrictive network environments where managed agent nodes may only have outbound internet access on ports `80` and `443`.

The production architecture is centralized and agent-based:

```text
Browser
  |
  | HTTPS + WebSocket
  v
Sunbolt Control Plane
  |
  | Sunbolt Native Outbound Agent Channel
  | Baseline: TLS over TCP/443
  | Optional: QUIC over UDP/443 when available
  v
Sunbolt Agent Node
  |
  | Local PTY
  v
Shell
```

The control plane remains the source of truth for:

- Identity
- Authentication
- Authorization
- MFA policy
- Node enrollment
- Node revocation
- Terminal session metadata
- Audit logs
- Route and transport policy

## Production Goals

1. Secure terminal access with server-side authorization on every operation.
2. Outbound-only agent connectivity that works through restrictive firewalls.
3. Stable terminal sessions that survive browser navigation and short disconnects.
4. Multi-tab terminal workspace with clear detach, close, and terminate semantics.
5. Durable production state in PostgreSQL.
6. Clear structured logging and audit events across system interactions.
7. Responsive and adaptive UI for mobile, tablet, laptop, and desktop.
8. Clean Rust crate/module structure with minimal duplication.
9. Production deployment guidance with explicit development and production modes.
10. Documentation for architecture, operations, transport, terminal lifecycle, and security.

## Runtime Modes

Sunbolt should have exactly two runtime modes:

- `development`
- `production`

Development mode may support local bootstrap credentials, permissive local origins, and in-memory scaffolding while features are being built.

Production mode must require explicit configuration and durable storage. Production must reject development-only shortcuts.

Preview and test environments should run as either development-like or production-like deployments using environment-specific configuration, not a separate application mode.

## Current Baseline

The repository already contains:

- Rust workspace with control, UI, agent, auth, terminal, protocol, audit, storage, and common crates.
- Local PTY terminal support.
- Browser WebSocket terminal protocol.
- Dioxus web UI with xterm.js integration.
- Initial auth, step-up MFA, RBAC, node enrollment, audit, and terminal lifecycle foundations.
- Agent node MVP concepts.
- Some detach/reattach protocol support.
- Production deployment notes and migration scaffolding.

The production phase must turn this MVP baseline into a durable, maintainable system. Existing in-memory registries and placeholder UI flows should be replaced or isolated behind production-ready boundaries.

## Target Architecture

### Control Plane

The control plane should be split into explicit modules:

- `config`: environment loading and runtime mode validation
- `error`: API and domain errors
- `routes`: Axum route definitions and handlers
- `state`: shared application state
- `auth`: request/session extraction and authorization checks
- `terminal`: browser terminal WebSocket orchestration
- `agent`: agent enrollment, connection ownership, heartbeat, commands
- `node`: node management and revocation
- `audit`: audit writer integration and event mapping
- `transport`: agent transport server implementations
- `repositories`: storage-backed control-plane data access

Large implementation should not remain concentrated in `sunbolt-control/src/lib.rs`.

### Storage

PostgreSQL should be mandatory for production.

Durable production state should include:

- Users
- Sessions
- Recent MFA timestamps or equivalent challenge state
- MFA factors
- RBAC roles and permissions
- Workspaces and memberships
- Nodes
- Node credentials
- Node heartbeats
- Node revocations
- Agent connection metadata
- Terminal session metadata
- Terminal detach/reattach state
- Audit logs
- Route health and ownership leases where needed

In-memory structures may cache live sockets, PTY handles, broadcast channels, output buffers, and short-lived runtime handles. They must not be the durable source of truth for production.

### Terminal Lifecycle

Terminal lifetime must be independent from browser page lifetime.

Browser page navigation, refresh, route changes, tab changes, mobile app backgrounding, and short WebSocket disconnects should detach the browser from the terminal session. They must not close the PTY.

The control plane and agent should model terminal states explicitly:

```text
Created -> Starting -> Active -> Detached -> Reattaching -> Active
Active -> Terminating -> Terminated
Active -> Failed
Detached -> Expired
```

The exact enum may differ, but the implementation must distinguish:

- UI tab closed
- Browser WebSocket disconnected
- User detached
- User explicitly terminated the terminal
- PTY process exited
- Backend policy closed the terminal
- Node disconnected
- Node revoked

Reattach must verify:

- Authenticated browser session
- User identity
- Workspace membership
- `terminal.reattach` or equivalent permission
- Terminal ownership or delegated access
- Session state allows reattach
- Node is still trusted and not revoked

The UI may use a reconnect token as an implementation detail, but production authorization must not depend only on a bearer reconnect token.

### Multi-Tab Terminal Workspace

The terminal workspace should support multiple terminal tabs.

Required semantics:

- Open new terminal tab.
- Switch active terminal tab.
- Rename or display useful terminal labels.
- Close UI tab without killing PTY.
- Detach terminal session.
- Reattach existing terminal session.
- Explicitly terminate terminal session.
- List active and detached sessions after page reload.
- Preserve terminal identity across route changes.

For mobile, terminal tabs should collapse into a compact session switcher, dropdown, segmented control, or bottom sheet.

### Agent Transport

Sunbolt should own the agent-control-plane transport. The core product must not depend on a third-party tunnel provider.

Agents initiate outbound connections to the control plane.

Production transport strategy:

1. Implement a transport abstraction.
2. Keep a development WebSocket transport for local iteration.
3. Implement a production baseline over TLS/TCP/443 using WebSocket or HTTP/2.
4. Add optional QUIC over UDP/443 as a fast path when the network allows it.
5. Add a restrictive-network fallback if required, such as long-poll or request/response control messages.

QUIC is valuable for multiplexed streams, connection migration, and transport-level flow control, but it cannot be the only production path because UDP is often blocked.

The transport layer should support:

- Protocol version negotiation
- Agent identity authentication
- Heartbeat
- Liveness timeout
- Backoff and reconnect
- Resume after transient disconnect
- Per-terminal stream routing
- Backpressure
- Message IDs
- Terminal output sequence numbers
- Transport metrics
- Structured logs

### Agent Identity

Enrollment starts with a one-time token.

After enrollment, the agent should use durable node identity material:

- Node ID
- Credential fingerprint
- Private key or certificate material
- Expiration metadata
- Rotation metadata
- Revocation state

Production should support or plan for:

- mTLS or equivalent authenticated handshake
- Credential rotation
- Node revocation
- Agent version tracking
- Heartbeat and health status
- Forced session shutdown on revocation
- Clear audit events for each identity lifecycle action

### Audit and Observability

Sunbolt should provide both auditability and operational visibility.

Audit logs answer: who did what, to which resource, when, and whether it was allowed.

Structured logs answer: how components interacted and why operations succeeded, retried, degraded, or failed.

Use `tracing` consistently with fields such as:

- `request_id`
- `actor_id`
- `actor_email`
- `workspace_id`
- `node_id`
- `session_id`
- `transport_id`
- `route_id`

Audit events should include:

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
- `agent.connected`
- `agent.disconnected`
- `agent.transport.negotiated`
- `node.enrolled`
- `node.revoked`
- `node.credential.rotated`
- `permission.changed`
- `route.selected`
- `route.failed`

Secrets must be redacted from logs and audit messages.

### UI Architecture

Dioxus should be organized around pages, reusable components, and shared client/state layers.

Recommended UI structure:

```text
sunbolt-ui/
  src/
    app.rs
    routes.rs
    api/
    components/
    layouts/
    pages/
    terminal/
    state/
    theme/
```

The exact structure may evolve, but the responsibilities should stay separate:

- Pages compose workflows.
- Components render reusable controls.
- API clients handle HTTP/WebSocket calls.
- Terminal state is shared across device layouts.
- Layouts adapt to desktop, tablet, and mobile.
- Styles are centralized and reused.

Avoid duplicating buttons, tables, badges, forms, modals, sheets, status indicators, and API calls.

### Responsive and Adaptive UI

Sunbolt must be responsive by default and adaptive where workflows change by device class.

Desktop and laptop:

- Full navigation.
- Dense data tables.
- Multi-tab terminal workspace.
- Terminal viewport should dominate terminal screens.
- Filtering, search, and pagination should be efficient.

Tablet:

- Two-pane views where width allows.
- Compact terminal controls.
- Node list and terminal/detail split views.
- Tables may become compact rows.

Mobile:

- Terminal-first full-screen workspace.
- Bottom navigation or compact top navigation.
- Terminal session switcher instead of wide tabs.
- Node selector and session actions in bottom sheets.
- Dense list rows instead of wide tables.
- Login and MFA flows must remain usable when the keyboard is open.

Mobile terminal accessory controls should include:

- `Ctrl`
- `Esc`
- `Tab`
- Arrow keys
- Paste
- Reconnect
- Detach
- Terminate session

Baseline validation viewports:

- iPhone 11 Pro: `375x812`
- iPad 11 Pro portrait: `834x1194`
- iPad 11 Pro landscape: `1194x834`
- Laptop: `1366x768`
- Desktop: `1920x1080`

### Security Hardening

Production must enforce:

- HTTPS at the public edge
- Secure HttpOnly cookies
- SameSite cookie policy
- CSRF protection for state-changing HTTP routes
- WebSocket origin validation
- Strong session expiration
- Step-up MFA policy for terminal open
- Server-side authorization on every route and WebSocket command
- Rate limits for login, MFA, enrollment, and terminal creation
- Node revocation enforcement
- Secret redaction in logs
- No development bootstrap admin
- No auth tokens in browser local storage
- No hidden production credentials

### Documentation

Documentation should evolve with the production system.

Required docs:

- Local development
- Production deployment
- Security model
- Agent enrollment
- Agent transport
- Terminal lifecycle
- Terminal reconnect/reattach
- UI architecture
- Audit event taxonomy
- Backup and restore
- Migration notes

## Production Phase Roadmap

### Phase 8.1: Documentation and Architecture Reset

Goal: make the production direction explicit and remove old MVP-first planning from the source of truth.

Deliverables:

- Rewritten `AGENTS.md`
- Rewritten `PLAN.md`
- Rewritten `TASK.md`
- Clear production principles
- Clear transport direction
- Clear responsive/adaptive UI direction

### Phase 8.2: Control-Plane Structure Cleanup

Goal: split large control-plane implementation into maintainable modules without changing behavior.

Deliverables:

- Route modules
- Terminal session modules
- Agent/node modules
- Config/error/state modules
- Focused tests preserved
- No unrelated behavior changes

### Phase 8.3: Durable Production State

Goal: move production-critical state out of process memory.

Deliverables:

- Storage-backed auth/session repository boundaries
- Storage-backed node and credential repositories
- Storage-backed terminal session metadata
- Storage-backed audit append path
- Migration coverage
- Production config validation

### Phase 8.4: Terminal Reattach and Multi-Tab Workspace

Goal: make terminal sessions durable across browser navigation and usable as a multi-tab workspace.

Deliverables:

- Active session listing API
- Detached session listing API
- Reattach flow for local and remote sessions
- Explicit terminate endpoint/message
- UI terminal tabs/session switcher
- Mobile terminal session switcher
- Tests for detach, reattach, and terminate semantics

### Phase 8.5: Sunbolt Native Agent Transport

Goal: build an outbound-only production transport foundation.

Deliverables:

- Agent transport trait
- Control-plane transport registry
- TCP/443 TLS transport baseline using WebSocket or HTTP/2
- Transport negotiation
- Heartbeat and reconnect
- Backpressure policy
- Message IDs and terminal output sequence numbers
- QUIC design spike and optional implementation plan

### Phase 8.6: Agent Identity and Revocation

Goal: make enrolled agents trustworthy beyond one-time token bootstrap.

Deliverables:

- Durable node identity
- Credential fingerprinting
- Rotation model
- Revocation enforcement
- Audit events
- Agent reconnect with identity verification

### Phase 8.7: Responsive Production UI

Goal: redesign the UI into reusable pages/components that work across device classes.

Deliverables:

- Shared design system components
- Desktop/laptop layout
- Tablet adaptive layout
- Mobile terminal-first layout
- Bottom sheets or compact controls for mobile workflows
- Terminal accessory toolbar for mobile
- No duplicated page/control logic
- Viewport validation across required device sizes

### Phase 8.8: Observability and Audit

Goal: make system interactions traceable and audit-ready.

Deliverables:

- Consistent tracing spans
- Correlation IDs
- Audit event taxonomy
- Secret redaction
- Agent transport logs
- Terminal lifecycle logs
- Audit export and chain verification path

### Phase 8.9: Security Hardening

Goal: remove production shortcuts and enforce security policy.

Deliverables:

- Runtime mode validation
- Production config validation
- CSRF enforcement
- Origin validation hardening
- Rate limit review
- Secure cookie review
- MFA policy review
- Authorization tests for routes and WebSocket commands
- Node revocation tests

### Phase 8.10: Production Validation

Goal: establish the release gate for production deployment.

Deliverables:

- `cargo test` passing
- `cargo clippy --all-targets --all-features -- -D warnings` passing
- `cargo fmt --all -- --check` passing
- Migration verification
- Basic load testing for terminal streams
- Agent reconnect testing
- Browser/device viewport testing
- Deployment runbook updated

## Success Criteria

The production phase is complete when:

- A production deployment can run without development shortcuts.
- Agents connect outbound through TCP/443.
- QUIC is available as an optional fast path or has a documented implementation spike.
- Terminal sessions survive browser route changes and short disconnects.
- Users can manage multiple terminal tabs/sessions.
- Mobile, tablet, laptop, and desktop layouts are usable.
- Audit logs and structured traces clearly describe system interactions.
- Production-critical state is durable.
- Code modules are split by responsibility.
- Documentation matches the implemented behavior.
- Required checks pass before release.
