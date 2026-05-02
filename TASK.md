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

- [x] Rename or initialize the project as `sunbolt`.
- [x] Create a Rust workspace.
- [x] Add root `Cargo.toml` workspace members.
- [x] Add `README.md` with the Sunbolt product description.
- [x] Add `AGENTS.md`, `PLAN.md`, and `TASK.md`.
- [x] Add `.gitignore` for Rust, Dioxus, local env files, and build artifacts.
- [x] Add `.env.example` for local development config.

## Workspace Crates / Modules

- [x] Create `crates/sunbolt-common`.
- [x] Create `crates/sunbolt-control`.
- [x] Create `crates/sunbolt-ui`.
- [x] Create `crates/sunbolt-terminal`.
- [x] Create `crates/sunbolt-auth`.
- [x] Create `crates/sunbolt-storage`.
- [x] Create `crates/sunbolt-audit`.
- [x] Create `crates/sunbolt-protocol`.
- [x] Defer `crates/sunbolt-agent` until Phase 3 unless needed earlier.

## Tooling

- [x] Ensure `cargo test` runs successfully.
- [x] Ensure `cargo clippy --all-targets --all-features -- -D warnings` runs successfully.
- [x] Add formatting guidance using `cargo fmt`.
- [x] Add Tailwind CSS tooling for web UI design.
- [x] Upgrade Axum to 0.8.
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
- [x] Add Sunbolt theme colors.
- [ ] Add placeholder dashboard page.
- [ ] Add placeholder terminal page.
- [ ] Add placeholder nodes page.
- [ ] Add placeholder access history page.

---

# Phase 1: Local Web Terminal MVP

## Terminal Backend

- [x] Choose PTY crate for local terminal spawning.
- [x] Implement `TerminalSessionId` type.
- [x] Implement terminal session states:
  - [x] `Created`
  - [x] `Starting`
  - [x] `Active`
  - [x] `Closing`
  - [x] `Closed`
  - [x] `Failed`
- [x] Implement local PTY spawn.
- [x] Implement terminal input write.
- [x] Implement terminal output read.
- [x] Implement terminal resize.
- [x] Implement terminal close.
- [x] Implement terminal process exit handling.
- [x] Add unit tests for session state transitions.

## WebSocket Terminal Stream

- [x] Add WebSocket endpoint for terminal connections.
- [x] Define browser-to-server terminal messages:
  - [x] `Start`
  - [x] `Input`
  - [x] `Resize`
  - [x] `Close`
  - [x] `Ping`
- [x] Define server-to-browser terminal messages:
  - [x] `Started`
  - [x] `Output`
  - [x] `Exited`
  - [x] `Error`
  - [x] `Pong`
- [x] Implement terminal input forwarding.
- [x] Implement terminal output forwarding.
- [x] Handle browser disconnect.
- [x] Handle backend PTY failure.
- [x] Add backpressure strategy or document temporary behavior.

## Terminal UI

- [x] Integrate xterm.js or chosen terminal emulator.
- [x] Create Dioxus terminal component wrapper.
- [x] Connect terminal input to WebSocket.
- [x] Render terminal output from WebSocket.
- [x] Send resize events to backend.
- [x] Add terminal connection status indicator.
- [x] Add close terminal button.
- [x] Add reconnect placeholder UI.
- [ ] Test on desktop browser.
- [ ] Test on iPhone/mobile browser.

## Session Tracking

- [x] Track active sessions in memory.
- [x] Add max sessions per process config.
- [x] Add idle timeout config.
- [x] Add graceful cleanup on close.
- [x] Add cleanup on backend shutdown.

---

# Phase 2: Storage, Auth, and Audit

## Database

- [x] Add PostgreSQL config.
- [x] Add SeaORM dependency.
- [x] Add migration setup.
- [x] Create `users` table.
- [x] Create `sessions` table.
- [x] Create `terminal_sessions` table.
- [x] Create `audit_logs` table.
- [x] Add storage crate APIs.

## Authentication

- [x] Add user model.
- [x] Add dev-only bootstrap admin creation.
- [x] Add login endpoint.
- [x] Add logout endpoint.
- [x] Add secure session cookie.
- [x] Add current-user endpoint.
- [x] Add auth middleware.
- [x] Protect terminal WebSocket endpoint with auth.

## Authorization

- [x] Add simple role enum for MVP:
  - [x] `Admin`
  - [x] `Operator`
  - [x] `Viewer`
- [x] Require `Admin` or `Operator` to open terminal.
- [x] Prevent `Viewer` from opening terminal.
- [x] Add server-side authorization tests.

## Audit Logs

- [x] Implement audit event writer.
- [x] Record login success.
- [x] Record login failure.
- [x] Record logout.
- [x] Record terminal opened.
- [x] Record terminal closed.
- [x] Record terminal failed.
- [x] Add access history page.
- [x] Add audit log page placeholder.

---

# Phase 3: Agent Node MVP

## Agent Binary

- [x] Create `crates/sunbolt-agent`.
- [x] Add agent config loading.
- [x] Add agent startup logs.
- [x] Add agent local node information collection:
  - [x] hostname
  - [x] OS
  - [x] architecture
  - [x] agent version
- [x] Add graceful shutdown.

## Node Enrollment

- [x] Add `nodes` table.
- [x] Add `node_credentials` table.
- [x] Add `node_heartbeats` table.
- [x] Add `enrollment_tokens` table.
- [x] Add create enrollment token endpoint.
- [x] Add enrollment command UI.
- [x] Add agent enrollment request.
- [x] Mark enrollment token as used.
- [x] Store node identity.

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
