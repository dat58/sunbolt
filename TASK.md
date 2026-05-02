# TASK.md

# Sunbolt Task List

This file tracks implementation tasks for Sunbolt.

Status markers:

- `[ ]` Not started
- `[~]` In progress
- `[x]` Done
- `[!]` Blocked / needs decision

## Current Priority

Build the local web terminal MVP before implementing distributed agent nodes.

Target first milestone:

```text
A user can open a browser, log in locally or through a temporary dev auth flow, open a terminal on the server running Sunbolt, type commands, see output, resize the terminal, and close the session.
```

---

# Phase 0: Project Foundation

## Repository Setup

- [ ] Rename or initialize the project as `sunbolt`.
- [ ] Create a Rust workspace.
- [ ] Add root `Cargo.toml` workspace members.
- [ ] Add `README.md` with the Sunbolt product description.
- [ ] Add `AGENTS.md`, `PLAN.md`, and `TASK.md`.
- [ ] Add `.gitignore` for Rust, Dioxus, local env files, and build artifacts.
- [ ] Add `.env.example` for local development config.

## Workspace Crates / Modules

- [ ] Create `crates/sunbolt-common`.
- [ ] Create `crates/sunbolt-control`.
- [ ] Create `crates/sunbolt-ui`.
- [ ] Create `crates/sunbolt-terminal`.
- [ ] Create `crates/sunbolt-auth`.
- [ ] Create `crates/sunbolt-storage`.
- [ ] Create `crates/sunbolt-audit`.
- [ ] Create `crates/sunbolt-protocol`.
- [ ] Defer `crates/sunbolt-agent` until Phase 3 unless needed earlier.

## Tooling

- [ ] Ensure `cargo test` runs successfully.
- [ ] Ensure `cargo clippy --all-targets --all-features -- -D warnings` runs successfully.
- [ ] Add formatting guidance using `cargo fmt`.
- [ ] Add basic CI workflow if GitHub Actions is used.
- [ ] Add local development command documentation.

## Backend Skeleton

- [ ] Add Axum backend binary or library entrypoint.
- [ ] Add Tokio runtime.
- [ ] Add `/health` endpoint.
- [ ] Add structured error type.
- [ ] Add request tracing/logging.
- [ ] Add config loading from environment variables.

## UI Skeleton

- [ ] Add Dioxus web app shell.
- [ ] Add base layout.
- [ ] Add Sunbolt theme colors.
- [ ] Add placeholder dashboard page.
- [ ] Add placeholder terminal page.
- [ ] Add placeholder nodes page.
- [ ] Add placeholder access history page.

---

# Phase 1: Local Web Terminal MVP

## Terminal Backend

- [ ] Choose PTY crate for local terminal spawning.
- [ ] Implement `TerminalSessionId` type.
- [ ] Implement terminal session states:
  - [ ] `Created`
  - [ ] `Starting`
  - [ ] `Active`
  - [ ] `Closing`
  - [ ] `Closed`
  - [ ] `Failed`
- [ ] Implement local PTY spawn.
- [ ] Implement terminal input write.
- [ ] Implement terminal output read.
- [ ] Implement terminal resize.
- [ ] Implement terminal close.
- [ ] Implement terminal process exit handling.
- [ ] Add unit tests for session state transitions.

## WebSocket Terminal Stream

- [ ] Add WebSocket endpoint for terminal connections.
- [ ] Define browser-to-server terminal messages:
  - [ ] `Start`
  - [ ] `Input`
  - [ ] `Resize`
  - [ ] `Close`
  - [ ] `Ping`
- [ ] Define server-to-browser terminal messages:
  - [ ] `Started`
  - [ ] `Output`
  - [ ] `Exited`
  - [ ] `Error`
  - [ ] `Pong`
- [ ] Implement terminal input forwarding.
- [ ] Implement terminal output forwarding.
- [ ] Handle browser disconnect.
- [ ] Handle backend PTY failure.
- [ ] Add backpressure strategy or document temporary behavior.

## Terminal UI

- [ ] Integrate xterm.js or chosen terminal emulator.
- [ ] Create Dioxus terminal component wrapper.
- [ ] Connect terminal input to WebSocket.
- [ ] Render terminal output from WebSocket.
- [ ] Send resize events to backend.
- [ ] Add terminal connection status indicator.
- [ ] Add close terminal button.
- [ ] Add reconnect placeholder UI.
- [ ] Test on desktop browser.
- [ ] Test on iPhone/mobile browser.

## Session Tracking

- [ ] Track active sessions in memory.
- [ ] Add max sessions per process config.
- [ ] Add idle timeout config.
- [ ] Add graceful cleanup on close.
- [ ] Add cleanup on backend shutdown.

---

# Phase 2: Storage, Auth, and Audit

## Database

- [ ] Add PostgreSQL config.
- [ ] Add SeaORM dependency.
- [ ] Add migration setup.
- [ ] Create `users` table.
- [ ] Create `sessions` table.
- [ ] Create `terminal_sessions` table.
- [ ] Create `audit_logs` table.
- [ ] Add storage crate APIs.

## Authentication

- [ ] Add user model.
- [ ] Add dev-only bootstrap admin creation.
- [ ] Add login endpoint.
- [ ] Add logout endpoint.
- [ ] Add secure session cookie.
- [ ] Add current-user endpoint.
- [ ] Add auth middleware.
- [ ] Protect terminal WebSocket endpoint with auth.

## Authorization

- [ ] Add simple role enum for MVP:
  - [ ] `Admin`
  - [ ] `Operator`
  - [ ] `Viewer`
- [ ] Require `Admin` or `Operator` to open terminal.
- [ ] Prevent `Viewer` from opening terminal.
- [ ] Add server-side authorization tests.

## Audit Logs

- [ ] Implement audit event writer.
- [ ] Record login success.
- [ ] Record login failure.
- [ ] Record logout.
- [ ] Record terminal opened.
- [ ] Record terminal closed.
- [ ] Record terminal failed.
- [ ] Add access history page.
- [ ] Add audit log page placeholder.

---

# Phase 3: Agent Node MVP

## Agent Binary

- [ ] Create `crates/sunbolt-agent`.
- [ ] Add agent config loading.
- [ ] Add agent startup logs.
- [ ] Add agent local node information collection:
  - [ ] hostname
  - [ ] OS
  - [ ] architecture
  - [ ] agent version
- [ ] Add graceful shutdown.

## Node Enrollment

- [ ] Add `nodes` table.
- [ ] Add `node_credentials` table.
- [ ] Add `node_heartbeats` table.
- [ ] Add `enrollment_tokens` table.
- [ ] Add create enrollment token endpoint.
- [ ] Add enrollment command UI.
- [ ] Add agent enrollment request.
- [ ] Mark enrollment token as used.
- [ ] Store node identity.

## Agent Connection

- [ ] Implement agent outbound connection to control plane.
- [ ] Add heartbeat message.
- [ ] Track node online/offline status.
- [ ] Display nodes in UI.
- [ ] Add node details page.
- [ ] Add revoke node action.

## Remote Terminal Through Agent

- [ ] Define control-plane-to-agent terminal protocol.
- [ ] Implement `StartTerminal` command.
- [ ] Implement `TerminalOutput` event.
- [ ] Implement `WriteInput` command.
- [ ] Implement `ResizeTerminal` command.
- [ ] Implement `CloseTerminal` command.
- [ ] Route browser WebSocket to selected agent.
- [ ] Open terminal on selected remote node.
- [ ] Handle agent disconnect during active terminal.

---

# Phase 4: MFA and RBAC

## MFA Foundation

- [ ] Create `AuthFactor` trait.
- [ ] Define `FactorType` enum.
- [ ] Define challenge and response types.
- [ ] Add `auth_factors` table.
- [ ] Add factor enrollment flow.
- [ ] Add factor verification flow.

## TOTP

- [ ] Add TOTP secret generation.
- [ ] Add QR code display.
- [ ] Add TOTP verification.
- [ ] Add TOTP recovery path.

## Recovery Codes

- [ ] Add recovery code generation.
- [ ] Store hashed recovery codes.
- [ ] Verify recovery code.
- [ ] Invalidate used recovery code.
- [ ] Add regenerate recovery codes action.

## WebAuthn / Passkeys

- [ ] Research WebAuthn crate choice.
- [ ] Add passkey registration challenge.
- [ ] Add passkey authentication challenge.
- [ ] Add passkey management UI.

## Step-up MFA

- [ ] Add policy requiring step-up MFA for terminal open.
- [ ] Add recent-MFA timestamp to session.
- [ ] Prompt MFA before opening terminal when required.
- [ ] Add audit event for MFA challenge and success.

## RBAC

- [ ] Add `workspaces` table.
- [ ] Add `workspace_members` table.
- [ ] Add `roles` table.
- [ ] Add `permissions` table.
- [ ] Add `role_permissions` table.
- [ ] Map nodes to workspaces.
- [ ] Add workspace-level permission checks.
- [ ] Add user/team management UI.

---

# Phase 5: Hardening and Reliability

## Terminal Reliability

- [ ] Add detach/reattach model.
- [ ] Keep PTY alive during short browser disconnect.
- [ ] Add reconnect token.
- [ ] Add session cleanup worker.
- [ ] Add per-user session limit.
- [ ] Add per-node session limit.
- [ ] Add terminal idle timeout.
- [ ] Add terminal absolute max duration.

## Audit Hardening

- [ ] Add append-only audit behavior.
- [ ] Add `previous_hash` column.
- [ ] Add `event_hash` column.
- [ ] Verify audit chain integrity.
- [ ] Add audit export.

## Security Hardening

- [ ] Add WebSocket origin validation.
- [ ] Add CSRF protection for state-changing HTTP routes.
- [ ] Add secure cookie settings for production.
- [ ] Add content security policy.
- [ ] Add rate limits for login.
- [ ] Add rate limits for terminal creation.
- [ ] Add node revocation enforcement.
- [ ] Add secret redaction in logs.

## Production Readiness

- [ ] Add Dockerfile.
- [ ] Add production config example.
- [ ] Add reverse proxy example.
- [ ] Add HTTPS deployment note.
- [ ] Add database migration command docs.
- [ ] Add backup/restore notes.

---

# Phase 6: Distributed Expansion

## Routing

- [ ] Add `NodeRouter` abstraction.
- [ ] Add route selection for direct agent connection.
- [ ] Add route selection for relay node.
- [ ] Track route health.

## Mesh Research

- [ ] Evaluate QUIC transport.
- [ ] Evaluate WireGuard overlay option.
- [ ] Evaluate node-to-node relay mode.
- [ ] Define trust model for node-to-node communication.
- [ ] Define audit implications of relay routing.

## Control Plane HA

- [ ] Identify state that must be shared.
- [ ] Evaluate Redis/NATS/Postgres notification channel for active session routing.
- [ ] Design multi-control-plane agent connection strategy.
- [ ] Design sticky routing for active WebSocket sessions.

---

# Bug / Issue Backlog

Add bugs here as they are discovered.

- [ ] No known bugs yet.

---

# Decision Log

## 2026-05-01

- [x] Project name changed to **Sunbolt**.
- [x] Prefer centralized control plane first.
- [x] Prefer agent-based distributed model later.
- [x] Do not start with pure P2P.
- [x] Use Dioxus primarily for UI.
- [x] Use Axum + Tokio for backend terminal/API logic.
- [x] Use PostgreSQL + SeaORM if persistent storage is needed.
- [x] Require `cargo test` and `cargo clippy --all-targets --all-features -- -D warnings` before every commit.
- [x] Require git add and git commit for every feature, bug fix, UI change, or refactor.
