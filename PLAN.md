# PLAN.md

# Sunbolt Planning Document

Sunbolt is a secure web-based remote terminal platform written primarily in Rust. The product starts as a single-server browser terminal and evolves into an agent-based distributed system for centralized management of many servers.

## Product Vision

Sunbolt should let authorized users open secure terminals to managed servers from any modern device:

- Laptop
- Desktop
- Tablet
- iPhone or other mobile browser

Long-term, Sunbolt should support multiple server nodes connected into a managed network where users can centrally view, manage, audit, and open terminals to any permitted node.

## Core Goals

1. Browser-based terminal access.
2. Secure authentication and MFA.
3. Centralized node/server management.
4. Distributed agent support.
5. Strong audit trail.
6. Permission-controlled terminal access.
7. Mobile-friendly and desktop-friendly UI.
8. Rust-first architecture using Dioxus, Tokio, Axum, and SeaORM.

## Non-Goals for the First MVP

The first MVP should not attempt to solve everything.

Do not build these first:

- Full P2P mesh
- Multi-region routing
- Terminal recording playback
- Complex RBAC UI
- Kubernetes integration
- Docker exec integration
- File manager
- Port forwarding
- SSH key manager
- Control plane high availability

Design boundaries should allow these later, but implementation should stay focused.

## Recommended Architecture

Use a centralized control plane first.

```text
Browser
  |
  | HTTPS + WebSocket
  v
Sunbolt Control Plane
  |
  | Local PTY in MVP
  | Agent protocol in later phase
  v
Server Terminal
```

Later distributed architecture:

```text
Browser
  |
  | HTTPS + WebSocket
  v
Sunbolt Control Plane
  |
  | Secure agent channel
  v
Sunbolt Agent Node
  |
  | Local PTY
  v
Shell
```

## Recommended Rust Workspace

Initial structure:

```text
sunbolt/
├── Cargo.toml
├── crates/
│   ├── sunbolt-control/
│   ├── sunbolt-ui/
│   ├── sunbolt-terminal/
│   ├── sunbolt-auth/
│   ├── sunbolt-storage/
│   ├── sunbolt-audit/
│   ├── sunbolt-protocol/
│   ├── sunbolt-agent/
│   └── sunbolt-common/
├── migrations/
├── AGENTS.md
├── PLAN.md
├── TASK.md
└── README.md
```

This can be simplified during the first commit if needed. The important thing is to avoid mixing UI, terminal, auth, storage, and protocol logic into one unstructured module.

## Major Components

## 1. Web UI

Recommended stack:

- Dioxus Web
- Tailwind or CSS modules if preferred
- xterm.js for terminal rendering

Main screens:

- Login
- MFA challenge
- Dashboard
- Nodes / Servers
- Terminal Workspace
- Terminal Sessions
- Access History
- Audit Logs
- Settings / Security

Design direction:

- Product name: Sunbolt
- Theme: sunlight + lightning
- Default terminal area should work well in dark mode

## 2. Control Plane

Recommended stack:

- Axum
- Tokio
- tower middleware
- WebSocket support
- SeaORM for database access

Responsibilities:

- Serve API
- Serve or integrate with Dioxus frontend
- Authenticate users
- Authorize actions
- Manage sessions
- Manage nodes
- Manage terminal sessions
- Route terminal streams
- Write audit logs
- Store access history

## 3. Terminal Core

Responsibilities:

- Spawn PTY
- Stream input/output
- Handle resize events
- Handle process exit
- Handle WebSocket disconnect
- Track session state
- Enforce idle timeout

Initial session states:

```text
Created -> Starting -> Active -> Closing -> Closed -> Failed
```

Later session states:

```text
Detached -> Reconnecting -> Reattached
```

## 4. Auth and MFA

Auth should be extensible from the start.

Suggested abstraction:

```rust
pub trait AuthFactor {
    fn factor_type(&self) -> FactorType;
    async fn begin_challenge(&self, ctx: &AuthContext) -> Result<Challenge, AuthError>;
    async fn verify_challenge(&self, ctx: &AuthContext, response: FactorResponse) -> Result<FactorResult, AuthError>;
}
```

Initial auth phases:

1. Basic local dev login.
2. Password-based login with secure password hashing.
3. TOTP factor.
4. Recovery codes.
5. WebAuthn/passkeys.
6. Step-up MFA for terminal access.

## 5. Authorization

Use resource-oriented permissions.

Example permissions:

```text
node.view
node.register
node.revoke
terminal.open
terminal.close
terminal.view_history
terminal.recording.view
audit.view
user.manage
role.manage
```

Initial MVP may use a simple admin/user role, but the data model should not block later workspace-level RBAC.

## 6. Storage

Use PostgreSQL with SeaORM migrations.

Initial tables:

```text
users
sessions
audit_logs
terminal_sessions
```

Phase 2 tables:

```text
nodes
node_credentials
node_heartbeats
enrollment_tokens
```

Phase 3 tables:

```text
auth_factors
recovery_codes
trusted_devices
workspaces
workspace_members
roles
permissions
role_permissions
```

## 7. Agent Node

The agent is a daemon installed on managed servers.

Responsibilities:

- Enroll with control plane
- Maintain heartbeat
- Hold a secure outbound connection to control plane
- Start local PTY sessions
- Stream terminal input/output
- Report terminal exit status
- Support certificate/key rotation later

Do not build the agent before the local terminal MVP works.

## 8. Node Communication

Preferred direction:

- MVP: no remote agent yet; local PTY only
- Phase 2: agent outbound WebSocket to control plane
- Phase 3: mTLS or signed node identity
- Phase 4: certificate rotation and revocation
- Phase 5: optional direct tunnel or mesh routing

## 9. Audit System

Audit logs should be append-only from the application perspective.

Events to record:

```text
user.login.success
user.login.failed
user.logout
user.mfa.challenge
terminal.opened
terminal.closed
terminal.failed
node.enrolled
node.revoked
permission.changed
```

Later events:

```text
terminal.recording.started
terminal.recording.stopped
node.cert.rotated
agent.upgraded
```

## 10. Security Model

Client to server:

- HTTPS only in production
- Secure HttpOnly cookies
- SameSite cookie policy
- CSRF protection where relevant
- WebSocket origin validation
- Short-lived terminal connection token

Control plane to agent:

- One-time enrollment token
- Node identity
- mTLS or equivalent secure channel
- Certificate rotation
- Node revocation

Data security:

- No plaintext passwords
- No hard-coded secrets
- No long-lived terminal tokens
- No auth tokens in localStorage
- Encrypt sensitive secrets at rest where needed

## Development Phases

## Phase 0: Project Foundation

Goal: create a clean Rust workspace and basic app skeleton.

Deliverables:

- Cargo workspace
- Basic Dioxus web app
- Basic Axum backend
- Shared config
- Health endpoint
- Basic error handling
- Initial README
- CI-ready commands

Validation:

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## Phase 1: Local Web Terminal MVP

Goal: open a terminal on the same server running Sunbolt.

Flow:

```text
Browser -> WebSocket -> Control Plane -> Local PTY
```

Deliverables:

- Terminal page
- xterm.js integration
- WebSocket endpoint
- Local PTY spawn
- Input/output streaming
- Resize support
- Close support
- Terminal session table
- Basic audit log

## Phase 2: Basic Auth and Access History

Goal: users can log in and terminal access is tracked.

Deliverables:

- User model
- Login/logout
- Secure session cookie
- Access history page
- Audit events for login and terminal access
- Basic role check for terminal access

## Phase 3: Agent Node MVP

Goal: open a terminal on another server running Sunbolt Agent.

Flow:

```text
Browser -> Control Plane -> Agent -> Local PTY
```

Deliverables:

- Agent binary
- Enrollment token
- Node registration
- Node heartbeat
- Node list page
- Open terminal on selected node
- Node offline handling

## Phase 4: MFA and RBAC

Goal: make terminal access safer and more granular.

Deliverables:

- Auth factor abstraction
- TOTP support
- Recovery codes
- Step-up MFA before opening terminal
- Workspace model
- Role/permission model
- Basic admin UI for user/node access

## Phase 5: Hardening

Goal: make the system safer and more reliable.

Deliverables:

- WebSocket backpressure handling
- Terminal reconnect/detach
- Idle timeout
- Session limits
- Node revocation
- Audit hash chain
- Configurable security policy
- Production deployment docs

## Phase 6: Distributed Expansion

Goal: support larger distributed deployments.

Deliverables:

- Multi-node routing abstraction
- Relay node support
- Direct tunnel exploration
- Optional WireGuard/QUIC transport research
- Control plane HA planning
- Agent auto-update planning

## Open Technical Questions

1. Which PTY crate should be used for the first implementation?
2. Should the first UI integrate xterm.js directly or through a small wrapper component?
3. Should backend and frontend be served from one binary during MVP?
4. Should sessions be stored server-side or as signed encrypted cookies?
5. Which password hashing crate and parameters should be used?
6. Should SeaORM migrations live inside a dedicated crate or root `migration/` directory?
7. Should agent protocol use JSON first for debugability, then binary later?
8. How strict should terminal recording be for the first auditable version?

## Recommended First Implementation Path

1. Create Rust workspace.
2. Create backend health endpoint.
3. Create Dioxus shell UI.
4. Create terminal page placeholder.
5. Add WebSocket endpoint.
6. Spawn local PTY.
7. Connect xterm.js to WebSocket.
8. Add resize support.
9. Add session lifecycle tracking.
10. Add minimal database schema.
11. Add audit logging.
12. Add basic login.
13. Add first node/agent work only after local terminal is stable.
