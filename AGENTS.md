# AGENTS.md

## Project: Sunbolt

Sunbolt is a Rust-first, production-oriented remote terminal and distributed server control platform.

The MVP is complete. The current product direction is to harden Sunbolt into a real production system that can safely manage terminals across many servers through a centralized control plane and outbound-only agent connections.

Target production flow:

```text
Browser -> Control Plane -> Sunbolt Native Outbound Agent Channel -> Agent Node -> PTY
```

The control plane remains the source of truth for identity, authentication, authorization, audit logs, node enrollment, policy, and terminal session metadata.

All documentation, code comments, identifiers, commit messages, and user-facing product text must be written in English.

## Core Principles

1. Security first.
2. Auditability by default.
3. Reliable terminal sessions over page-level convenience.
4. Rust-first implementation.
5. Explicit architecture over framework magic.
6. Small, testable, incremental changes.
7. Production state belongs in durable storage, not process memory.
8. Agent nodes must not require inbound firewall access.
9. UI must work well on mobile, tablet, laptop, and desktop.

## Runtime Modes

Sunbolt has exactly two runtime modes:

- `development`
- `production`

Development mode may include local bootstrap flows and in-memory shortcuts while a feature is under construction.

Production mode must not include development shortcuts. In production:

- Do not create hidden default admin credentials.
- Do not enable development bootstrap admin accounts.
- Do not allow permissive wildcard browser origins.
- Do not store auth tokens in `localStorage`.
- Do not accept plaintext node credentials.
- Do not use in-memory state as the durable source of truth.
- Do not expose terminal routes without TLS at the public edge.

Preview and test deployments should use one of these two modes with different configuration, secrets, domains, and infrastructure. Do not add a third mode unless there is a strong architectural reason.

## Technology Direction

Use the following stack unless there is a strong reason to change it:

- Language: Rust
- UI: Dioxus Web
- Backend API: Axum
- Async runtime: Tokio
- Database: PostgreSQL
- ORM/migrations: SeaORM / SeaORM migration
- Browser terminal transport: WebSocket over HTTPS
- Terminal rendering in browser: xterm.js or equivalent JS terminal emulator integrated into Dioxus
- Agent runtime: Rust daemon
- Agent-control-plane transport: Sunbolt-native outbound transport
- Serialization: serde first; version all protocol messages
- Auth/session: secure server-side sessions or secure HttpOnly cookies
- MFA design: abstract factor pipeline with support for WebAuthn/passkeys, TOTP, recovery codes, and future factors

## Architecture Rules

### Dioxus Usage

Dioxus owns the web UI. Do not hide critical backend behavior inside Dioxus server functions if it makes terminal, session, auth, node, or audit logic harder to test.

Prefer explicit Axum backend boundaries for:

- Authentication
- Authorization
- WebSocket terminal sessions
- Agent communication
- Node enrollment and revocation
- Audit logging
- Database access
- Terminal session lifecycle management

### Backend Boundaries

Keep crate and module responsibilities explicit:

- `sunbolt-ui`: web UI shell, pages, components, browser integration
- `sunbolt-control`: control-plane HTTP, WebSocket, routing, policy orchestration
- `sunbolt-agent`: managed node daemon and outbound control-plane channel
- `sunbolt-auth`: authentication, MFA, sessions, RBAC policy primitives
- `sunbolt-terminal`: PTY/session primitives and terminal lifecycle state
- `sunbolt-protocol`: versioned browser/control/agent protocol messages
- `sunbolt-audit`: audit event types, append-only writing, chain verification, export
- `sunbolt-storage`: PostgreSQL connection, repository boundaries, storage errors
- `sunbolt-common`: shared identifiers and small cross-cutting helpers

Do not place substantial implementation in `lib.rs`. Use `lib.rs` mainly for module declarations and public re-exports.

When a crate grows beyond a small skeleton, split responsibility-based modules such as:

- `config.rs`
- `error.rs`
- `routes/`
- `state.rs`
- `session.rs`
- `registry.rs`
- `service.rs`
- `transport/`
- `repository/`
- `protocol.rs`

Avoid duplicate code. Shared UI controls, request clients, status components, table/list renderers, terminal state, auth helpers, and protocol conversion logic should be extracted once and reused.

## Agent Transport Rules

Sunbolt owns the agent-control-plane connection. Do not make the core transport depend on a third-party tunnel provider.

Agent connections must be outbound-only from the node to the control plane. Production deployments must work when an agent node can only reach the internet through outbound ports `80` and `443`.

Transport priorities:

1. Baseline production transport: TLS over TCP/443 using WebSocket or HTTP/2.
2. Optional fast path: QUIC over UDP/443 when the network allows it.
3. Fallback for restrictive networks: long-poll or request/response control channel where needed.

Implement a transport abstraction before locking terminal/session logic to a concrete transport.

The transport layer must support or be designed to support:

- Version negotiation
- Authenticated node identity
- Heartbeats and liveness checks
- Backoff and reconnect
- Resume after transient disconnect
- Backpressure
- Message IDs and terminal output sequence numbers
- Transport-level metrics
- Structured logs with `node_id`, `transport_id`, and `session_id`

QUIC is useful but must not be the only production path because many enterprise networks block UDP.

## Node Security Rules

Node enrollment must use a one-time enrollment token.

Long-term node trust must use node identity material, not permanent shared passwords.

Production node identity should support:

- Node private key or certificate material
- mTLS or an equivalent authenticated handshake
- Credential expiration
- Credential rotation
- Node revocation
- Agent version tracking
- Heartbeat and health status
- Explicit audit logs for enrollment, connection, rotation, revocation, and disconnect

Revoked nodes must be unable to open or continue terminal sessions.

## Terminal Rules

Remote terminals are high-risk functionality.

Every terminal open request must pass through:

1. Authentication check
2. Authorization check
3. Optional step-up MFA policy check
4. Audit log write
5. Session lifecycle tracking
6. Durable terminal metadata write

Browser navigation, page refresh, tab switch, route change, or short WebSocket disconnect must detach from a terminal session. These events must not kill the PTY.

Only an explicit terminal terminate action, backend timeout, policy enforcement, node revocation, process exit, or administrative action may close the PTY.

Terminal sessions must support or be designed to support:

- Input stream
- Output stream
- Resize events
- Close and terminate events
- Detach
- Reattach
- Browser reconnect
- Multiple UI tabs per user workspace
- Idle timeout
- Absolute max duration
- Max session limit per user
- Max session limit per node
- Session ownership and authorization checks on every reattach
- Optional short output replay buffer using sequence numbers
- Optional session recording in a later phase

Terminal session metadata belongs in durable storage for production. In-memory maps may hold live handles, sockets, and PTY references, but they must not be the only source of truth.

## Auth and MFA Rules

Do not hard-code authentication as only username/password.

Design MFA as an extensible factor system. Supported or planned factors include:

- Password
- WebAuthn/passkey
- TOTP
- Recovery code
- Email OTP
- Hardware key
- Admin approval
- SSH key signature

Opening a production terminal must be compatible with step-up MFA.

Never rely on UI button hiding for security. Every backend route and WebSocket command must verify permissions server-side.

Use resource-oriented permissions such as:

- `node.view`
- `node.register`
- `node.revoke`
- `terminal.open`
- `terminal.close`
- `terminal.reattach`
- `terminal.view_history`
- `terminal.recording.view`
- `audit.view`
- `user.manage`
- `role.manage`

Prefer a workspace/project model for grouping nodes and users.

## Audit and Logging Rules

Audit logs must be append-only from the application perspective.

Security-relevant actions must have audit records. Operational interactions must have structured logs.

Use `tracing` spans and fields consistently. Include identifiers when available:

- `request_id`
- `actor_id`
- `actor_email`
- `workspace_id`
- `node_id`
- `session_id`
- `transport_id`
- `route_id`

Audit events should cover:

- User login success/failure
- MFA challenge and success/failure
- Terminal opened
- Terminal detached
- Terminal reattached
- Terminal terminated
- Terminal failed
- Agent connected
- Agent disconnected
- Transport negotiated
- Node enrolled
- Node revoked
- Node credential rotated
- Permission changed
- Route selected
- Route failed

Do not log secrets. Redact tokens, credentials, recovery codes, passkey material, cookies, and opaque long-lived identifiers.

## UI/UX Direction

The product name is **Sunbolt**.

The visual identity combines:

- Sunlight: warmth, trust, clarity
- Lightning: power, speed, responsiveness

Recommended palette:

- Sun amber: `#FBBF24`
- Warm orange: `#F59E0B`
- Soft sunlight background: `#FFF7ED`
- Electric violet: `#7C3AED`
- Electric blue: `#2563EB`
- Lightning cyan: `#22D3EE`

Prefer dark mode for terminal-heavy screens:

- Background: `#09090B`
- Surface: `#18181B`
- Border: `#27272A`
- Text: `#FAFAFA`
- Muted text: `#A1A1AA`

Main UI areas:

- Login / MFA
- Dashboard
- Server / Node Management
- Terminal Workspace
- Terminal Sessions
- Access History
- Audit Logs
- Users / Teams / Roles
- Settings / Security

### Responsive and Adaptive UI

Sunbolt must work across mobile, tablet, laptop, and desktop devices.

Use responsive layout by default and adaptive workflows where the device changes how the product should be used.

Desktop and laptop:

- Dense control-plane layout
- Full navigation
- Multi-tab terminal workspace
- Tables with filtering, search, and pagination
- Terminal viewport uses most of the available screen

Tablet:

- Two-pane layouts where width allows
- Compact terminal controls
- Node list and terminal/detail split views
- Tables may become compact rows

Mobile:

- Terminal-first full-screen workspace
- Bottom navigation or compact top navigation
- Terminal tabs represented by a dropdown, segmented control, or bottom sheet
- Node selector and session list in a bottom sheet
- Dense list rows instead of wide tables
- Login, MFA, detach, close, and terminate actions must remain reachable with the mobile keyboard open

Mobile terminal screens should provide an accessory toolbar for keys and actions that mobile keyboards handle poorly:

- `Ctrl`
- `Esc`
- `Tab`
- Arrow keys
- Paste
- Resize/reconnect
- Detach
- Terminate session

Close tab and terminate session must be distinct actions. Closing a UI tab must not kill the terminal unless the user explicitly chooses to terminate the session.

## Coding Standards

- Write idiomatic Rust.
- Prefer explicit error types over stringly errors.
- Use `thiserror` for library/domain errors when useful.
- Use `anyhow` only at application boundaries or binaries.
- Keep async boundaries clear.
- Avoid blocking calls inside async tasks unless wrapped properly.
- Avoid global mutable state.
- Prefer dependency injection through structs and traits.
- Keep protocol types versionable and serializable.
- Avoid premature optimization, but design for backpressure in terminal streams.
- Keep modules focused and testable.
- Avoid duplicate code.
- Keep public APIs small and explicit.
- Do not introduce unsafe code.

## Testing Rules

Every meaningful code change must include or update tests when practical.

At minimum, before committing any change, run:

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

If formatting rules are active, also run:

```bash
cargo fmt --all -- --check
```

For UI changes, validate the relevant device classes. Required baseline viewports:

- iPhone 11 Pro: `375x812`
- iPad 11 Pro portrait: `834x1194`
- iPad 11 Pro landscape: `1194x834`
- Laptop: `1366x768`
- Desktop: `1920x1080`

UI validation must check:

- Navigation does not break.
- Terminal is usable.
- Session switching works.
- No controls or text overlap.
- Login and MFA flows remain usable.
- Close tab, detach, and terminate semantics are clear.

## Mandatory Git Rule

For every code change that creates a feature, fixes a bug, changes UI, refactors code, changes schema, changes protocol, or updates behavior:

1. Run `cargo test`.
2. Run `cargo clippy --all-targets --all-features -- -D warnings`.
3. Ensure both commands pass without errors.
4. Run `git status` and review the changed files.
5. Stage the intended files with `git add`.
6. Create a git commit with a clear message.

Do not commit if `cargo test` or `cargo clippy` fails.

Do not include unrelated files in the commit.

Use focused commit messages, for example:

```text
feat(auth): add MFA factor trait
fix(terminal): handle websocket close event
refactor(protocol): split node messages by direction
ui(nodes): add server status table
security(agent): rotate node credentials
docs(plan): define production transport roadmap
```

## Commit Message Style

Prefer conventional commit prefixes:

- `feat:` for new features
- `fix:` for bug fixes
- `refactor:` for internal code changes
- `ui:` for UI-only changes
- `test:` for tests
- `docs:` for documentation
- `chore:` for tooling or maintenance
- `db:` for schema/migration changes
- `security:` for security hardening

## Documentation Rules

Update documentation when behavior, architecture, deployment, protocol, security posture, or operations change.

Relevant docs include:

- `README.md`
- `PLAN.md`
- `TASK.md`
- API/protocol docs
- Agent transport docs
- Terminal lifecycle docs
- Migration notes
- Security notes
- Deployment notes
- UI architecture notes

## Safety Rules

Because Sunbolt can open real server terminals, avoid shortcuts that weaken security.

Never intentionally add:

- Plaintext password storage
- Long-lived terminal tokens
- Auth tokens in `localStorage`
- Backend authorization bypasses
- Unsafe command execution from user-controlled input
- Hidden default admin credentials
- Hard-coded production secrets
- Unredacted secret logging
- Production behavior that depends on in-memory-only identity, audit, or terminal metadata

Development-only shortcuts must be clearly marked, isolated, and disabled in production mode.
