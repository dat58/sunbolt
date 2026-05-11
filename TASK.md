# TASK.md

# Sunbolt Production Task List

This file tracks the next production phase for Sunbolt.

All task descriptions, documentation, code comments, identifiers, commit messages, and user-facing product text must be written in English.

Status markers:

- `[ ]` Not started
- `[~]` In progress
- `[x]` Done
- `[!]` Blocked / needs decision

## Current Priority

The MVP is complete. The current priority is to harden Sunbolt into a production-ready remote terminal and distributed server control product.

Target production milestone:

```text
An authenticated and authorized user can open multiple browser terminal sessions
to managed agent nodes, navigate away or switch pages without killing PTYs,
reattach to active sessions, and rely on clear audit logs, stable outbound agent
connectivity, secure production configuration, and responsive UI across mobile,
tablet, laptop, and desktop devices.
```

Target production agent flow:

```text
Browser -> Control Plane -> Sunbolt Native Outbound Agent Channel -> Agent Node -> PTY
```

Agent nodes must not require inbound firewall access. Production baseline transport must work over outbound TCP/443. QUIC over UDP/443 may be added as an optional fast path, but it must not be the only production transport.

---

# MVP Baseline

The previous MVP phases are considered complete background for this production phase.

- [x] Rust workspace established.
- [x] Control-plane Axum foundation established.
- [x] Dioxus UI foundation established.
- [x] Local PTY terminal MVP established.
- [x] Browser WebSocket terminal protocol established.
- [x] Initial auth, MFA, RBAC, audit, and node concepts established.
- [x] Initial agent MVP concepts established.
- [x] Initial hardening and deployment notes established.
- [x] Initial terminal UI integration established.

Open MVP-era gaps must now be handled through the production tasks below, not by extending the old MVP roadmap.

---

# Phase 8.1: Documentation and Architecture Reset

Goal: replace the MVP-first planning documents with production-first guidance.

## Planning Documents

- [x] Rewrite `AGENTS.md` for the production phase.
- [x] Rewrite `PLAN.md` for the production phase.
- [x] Rewrite `TASK.md` for the production phase.
- [x] Document that all docs and code-facing text must be written in English.
- [x] Document exactly two runtime modes: `development` and `production`.
- [x] Document that Sunbolt owns the agent-control-plane transport.
- [x] Remove third-party tunnel dependency from the core transport plan.
- [x] Document outbound-only agent connectivity.
- [x] Document TCP/443 as the baseline production agent transport.
- [x] Document QUIC as an optional fast path, not a required-only path.
- [x] Document responsive and adaptive UI requirements.
- [x] Preserve mandatory git workflow guidance.
- [x] Preserve conventional commit style guidance.

## Follow-up Documentation

- [x] Update `README.md` to describe the production direction.
- [x] Update `docs/local-development.md` to distinguish development-only shortcuts from production behavior.
- [x] Update `docs/deployment.md` for the two runtime modes.
- [x] Add `docs/security-model.md`.
- [x] Add `docs/terminal-lifecycle.md`.
- [x] Add `docs/agent-transport.md`.
- [x] Add `docs/quic-fast-path.md`.
- [x] Add `docs/ui-architecture.md`.
- [x] Add `docs/audit-events.md`.

---

# Phase 8.2: Control-Plane Module Cleanup

Goal: split large control-plane implementation into maintainable modules without changing behavior.

## Module Structure

- [x] Split `sunbolt-control` config loading into `config.rs`.
- [x] Split control-plane error types into `error.rs`.
- [x] Split route construction into `routes/`.
- [x] Split application state into `state.rs`.
- [x] Split auth middleware and request extraction into `auth.rs` or `routes/auth.rs`.
- [x] Split terminal WebSocket orchestration into `terminal/`.
- [x] Split terminal session registry logic into `terminal/session_registry.rs`.
- [x] Split agent enrollment and heartbeat logic into `agent/`.
- [x] Split node management and revocation logic into `node/`.
- [x] Split audit integration helpers into `audit.rs`.
- [x] Keep `lib.rs` focused on module declarations and public re-exports.

## Behavior Preservation

- [x] Preserve existing route paths.
- [x] Preserve existing protocol messages.
- [x] Preserve existing tests during the refactor.
- [x] Add focused module-level tests where code moves reveal untested behavior.
- [x] Avoid unrelated behavior changes in the refactor commit.

---

# Phase 8.3: Persistent Production State

Goal: move production-critical state to PostgreSQL-backed repositories.

## Storage Boundaries

- [x] Define repository traits or service boundaries for users and sessions.
- [x] Define repository boundaries for MFA factor state.
- [x] Define repository boundaries for RBAC and workspace membership.
- [x] Define repository boundaries for nodes.
- [x] Define repository boundaries for node credentials.
- [x] Define repository boundaries for node heartbeats.
- [x] Define repository boundaries for terminal session metadata.
- [x] Define repository boundaries for audit events.
- [x] Keep live socket/PTY handles in memory only as runtime handles.

## Production State Migration

- [x] Ensure users are read from durable storage in production.
- [x] Ensure auth sessions are durable or backed by a production-grade session store.
- [x] Ensure recent MFA state is durable or recoverable.
- [x] Ensure nodes and credentials are durable.
- [x] Ensure terminal session metadata is durable.
- [x] Ensure audit logs are durable and append-only from the application perspective.
- [x] Ensure startup fails clearly in production when required storage config is missing.

## Tests

- [x] Add repository unit tests using mockable boundaries where practical.
- [x] Add migration tests where practical.
- [x] Add production config validation tests.
- [x] Add tests proving production mode does not rely on development bootstrap admin.

---

# Phase 8.4: Terminal Session Persistence, Reattach, and Multi-Tab Workspace

Goal: make terminal sessions durable across browser navigation and usable through a multi-tab workspace.

## Terminal Lifecycle Semantics

- [x] Define explicit terminal lifecycle states for production.
- [x] Distinguish UI tab close from PTY termination.
- [x] Distinguish browser WebSocket disconnect from PTY termination.
- [x] Distinguish detach from terminate.
- [x] Distinguish PTY process exit from backend policy close.
- [x] Add audit events for terminal detach.
- [x] Add audit events for terminal reattach.
- [x] Add audit events for explicit terminal terminate.

## Backend APIs and Protocol

- [x] Add API to list active terminal sessions for the authenticated user.
- [x] Add API to list detached terminal sessions for the authenticated user.
- [x] Add API or protocol command to explicitly terminate a terminal session.
- [x] Ensure reattach verifies authenticated user identity.
- [x] Ensure reattach verifies workspace membership and terminal permission.
- [x] Ensure reattach rejects revoked nodes.
- [x] Add terminal output sequence numbers.
- [x] Add a short replay buffer for recent terminal output where practical.
- [x] Add clear expired-session errors.

## Local Terminal Reattach

- [x] Keep local PTY alive when browser route changes.
- [x] Reattach local terminal by session identity after route changes.
- [x] Reattach local terminal after short WebSocket disconnect.
- [x] Ensure idle timeout still applies to detached sessions.
- [x] Ensure absolute max duration still applies to detached sessions.

## Remote Terminal Reattach

- [x] Keep remote agent PTY alive when browser route changes.
- [x] Reattach remote terminal through control plane after route changes.
- [x] Reattach remote terminal after short browser disconnect.
- [x] Handle agent disconnect during detached terminal state.
- [x] Handle agent reconnect and terminal resume where supported.
- [x] Return clear error when remote terminal cannot be recovered.

## Multi-Tab UI

- [x] Add terminal workspace tab model.
- [x] Add open new terminal action.
- [x] Add switch terminal tab action.
- [x] Add close UI tab action that does not kill PTY by default.
- [x] Add explicit terminate terminal action.
- [x] Add detached session list.
- [x] Add reattach session action.
- [x] Preserve active session identity across route changes.
- [x] Restore session list after browser reload.

## Tests

- [x] Test browser detach does not close PTY.
- [x] Test explicit terminate closes PTY.
- [x] Test reattach succeeds for authorized owner.
- [x] Test reattach fails for unauthorized user.
- [x] Test reattach fails for revoked node.
- [x] Test multiple sessions per user follow configured limits.
- [x] Test close UI tab and terminate session have different effects.

---

# Phase 8.5: Sunbolt Native Outbound Agent Transport

Goal: implement a production transport foundation owned by Sunbolt.

## Transport Abstraction

- [x] Define an `AgentTransport` trait or equivalent boundary.
- [x] Define transport connection lifecycle states.
- [x] Define transport error types.
- [x] Define transport metrics fields.
- [x] Define transport negotiation messages.
- [x] Define transport heartbeat messages.
- [x] Define transport reconnect behavior.
- [x] Define message ID requirements.
- [x] Define terminal stream sequence number requirements.

## Baseline TCP/443 Transport

- [x] Implement production baseline over TLS/TCP/443 using WebSocket or HTTP/2.
- [x] Ensure agent initiates the connection outbound.
- [x] Ensure no inbound access to the agent node is required.
- [x] Add heartbeat from agent to control plane.
- [x] Add control-plane liveness timeout.
- [x] Add agent reconnect with backoff.
- [x] Add control-plane detection for duplicate node connections.
- [x] Add transport negotiation audit event.
- [x] Add agent connected audit event.
- [x] Add agent disconnected audit event.

## Restrictive Network Fallback

- [x] Evaluate whether long-poll fallback is required.
- [x] Document fallback tradeoffs if not implemented immediately.
- [x] Ensure terminal UX reports degraded transport clearly when fallback is active.

## Optional QUIC Fast Path

- [x] Create a QUIC design spike.
- [x] Evaluate Rust QUIC implementation choices.
- [x] Confirm UDP/443 fallback behavior when blocked.
- [x] Define how QUIC maps terminal streams and control messages.
- [x] Implement QUIC only after the transport abstraction is stable.

## Tests

- [x] Test transport negotiation.
- [x] Test heartbeat timeout.
- [x] Test agent reconnect.
- [x] Test duplicate connection handling.
- [x] Test terminal command routing through the baseline transport.
- [x] Test backpressure behavior.

---

# Phase 8.6: Agent Identity, Rotation, and Revocation

Goal: move from enrollment-token bootstrap to production node trust.

## Enrollment and Identity

- [x] Keep one-time enrollment token flow.
- [x] Generate or register durable node identity during enrollment.
- [x] Store node credential fingerprint.
- [x] Store credential expiration metadata.
- [x] Store agent version metadata.
- [x] Persist node identity material safely on the agent host.
- [x] Document agent identity file permissions.

## Authentication

- [x] Authenticate every agent connection using durable node identity.
- [x] Reject unknown node identity.
- [x] Reject expired node credentials.
- [x] Reject revoked nodes.
- [x] Audit failed agent authentication.

## Rotation and Revocation

- [x] Add node credential rotation model.
- [x] Add rotation endpoint or command path.
- [x] Add audit event for credential rotation.
- [x] Enforce revocation on new connections.
- [x] Enforce revocation on active connections.
- [x] Terminate or detach active terminal sessions when a node is revoked.
- [x] Add audit events for forced terminal closure due to revocation.

## Tests

- [x] Test enrollment token is one-time.
- [x] Test durable node identity authenticates.
- [x] Test invalid credential is rejected.
- [x] Test revoked node cannot reconnect.
- [x] Test active node revocation closes or blocks terminal sessions.
- [x] Test credential rotation preserves authorized node access.

---

# Phase 8.7: Responsive and Adaptive Production UI

Goal: redesign the UI into reusable pages and components that work across device classes.

## UI Structure

- [x] Split `sunbolt-ui/src/lib.rs` into focused modules.
- [x] Add reusable layout components.
- [x] Add reusable button components.
- [x] Add reusable status badge components.
- [x] Add reusable table/list components.
- [x] Add reusable form components.
- [x] Add reusable modal/dialog components.
- [x] Add reusable bottom sheet components for mobile.
- [x] Add centralized API client module.
- [x] Add terminal workspace state module.
- [x] Move large browser bridge code into a maintainable asset/module boundary.

## Desktop and Laptop

- [x] Add full navigation layout.
- [x] Add dense dashboard layout.
- [x] Add multi-tab terminal workspace.
- [x] Add node management table with search/filter affordances.
- [x] Add audit and access history table views.
- [x] Ensure terminal viewport gets primary screen space.

## Tablet

- [ ] Add adaptive two-pane layout where width allows.
- [ ] Add node list plus detail/terminal split view.
- [ ] Compact terminal toolbar controls.
- [ ] Ensure portrait and landscape layouts are usable.

## Mobile

- [ ] Add terminal-first full-screen mobile workspace.
- [ ] Add bottom navigation or compact top navigation.
- [ ] Add mobile session switcher.
- [ ] Add node selector bottom sheet.
- [ ] Add session actions bottom sheet.
- [ ] Add mobile MFA/login layout that survives keyboard overlap.
- [ ] Add mobile terminal accessory toolbar.
- [ ] Include `Ctrl`, `Esc`, `Tab`, arrow keys, paste, reconnect, detach, and terminate controls.
- [ ] Replace wide tables with dense list rows on mobile.

## Required Viewport Validation

- [ ] Validate iPhone 11 Pro viewport: `375x812`.
- [ ] Validate iPad 11 Pro portrait viewport: `834x1194`.
- [ ] Validate iPad 11 Pro landscape viewport: `1194x834`.
- [ ] Validate laptop viewport: `1366x768`.
- [ ] Validate desktop viewport: `1920x1080`.

For each viewport:

- [ ] Navigation does not break.
- [ ] Terminal is usable.
- [ ] Session switching works.
- [ ] Text and controls do not overlap.
- [ ] Login flow is usable.
- [ ] MFA flow is usable.
- [ ] Close tab, detach, and terminate semantics are clear.

---

# Phase 8.8: Observability, Audit, and Operational Logging

Goal: make all major system interactions transparent and diagnosable.

## Structured Tracing

- [ ] Add request ID generation or propagation.
- [ ] Add tracing fields for `request_id`.
- [ ] Add tracing fields for `actor_id` or `actor_email`.
- [ ] Add tracing fields for `node_id`.
- [ ] Add tracing fields for `session_id`.
- [ ] Add tracing fields for `transport_id`.
- [ ] Add tracing fields for `route_id` when routing exists.
- [ ] Add spans for terminal open/detach/reattach/terminate.
- [ ] Add spans for agent connect/disconnect/reconnect.
- [ ] Add spans for transport negotiation.

## Audit Taxonomy

- [ ] Add `terminal.detached`.
- [ ] Add `terminal.reattached`.
- [ ] Add `terminal.terminated`.
- [ ] Add `agent.connected`.
- [ ] Add `agent.disconnected`.
- [ ] Add `agent.transport.negotiated`.
- [ ] Add `node.credential.rotated`.
- [ ] Add `route.selected`.
- [ ] Add `route.failed`.
- [ ] Document audit event schema.
- [ ] Document which events are security audit events versus operational logs.

## Secret Redaction

- [ ] Review current redaction rules.
- [ ] Redact cookies.
- [ ] Redact enrollment tokens.
- [ ] Redact node credentials.
- [ ] Redact recovery codes.
- [ ] Redact passkey credential material.
- [ ] Add tests for redaction coverage.

---

# Phase 8.9: Security Hardening

Goal: remove production shortcuts and enforce security policy.

## Runtime Mode Validation

- [ ] Add explicit `SUNBOLT_ENV`.
- [ ] Accept only `development` or `production`.
- [ ] Fail production startup if required secrets are missing.
- [ ] Fail production startup if development bootstrap admin is enabled.
- [ ] Fail production startup if wildcard origins are configured.
- [ ] Fail production startup if secure cookies are disabled.
- [ ] Document production config requirements.

## Browser Security

- [ ] Review Content Security Policy.
- [ ] Review WebSocket origin validation.
- [ ] Review CORS behavior.
- [ ] Enforce CSRF protection for state-changing HTTP routes.
- [ ] Ensure auth tokens are never stored in `localStorage`.
- [ ] Ensure session cookies are HttpOnly and Secure in production.

## Authorization

- [ ] Audit every HTTP route for server-side authorization.
- [ ] Audit every WebSocket command for server-side authorization.
- [ ] Add permission for terminal reattach.
- [ ] Add permission for terminal terminate.
- [ ] Add permission for node credential rotation.
- [ ] Add tests for viewer/operator/admin boundaries.
- [ ] Add tests for workspace-level terminal access.

## Rate Limits and Abuse Controls

- [ ] Review login rate limits.
- [ ] Add MFA challenge rate limits.
- [ ] Review terminal creation rate limits.
- [ ] Add enrollment token creation rate limits.
- [ ] Add agent authentication failure rate limits where practical.

---

# Phase 8.10: Production Validation and Release Gate

Goal: define the checks required before treating a build as production-ready.

## Required Local Checks

- [ ] Run `cargo test`.
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run UI build checks when frontend tooling is touched.
- [ ] Run migration verification when database schema changes.
- [ ] Run `git status` and review changed files.

## Terminal Reliability Validation

- [ ] Validate local terminal open/input/output/resize/terminate.
- [ ] Validate remote terminal open/input/output/resize/terminate.
- [ ] Validate browser route change does not kill PTY.
- [ ] Validate browser refresh can recover active sessions where supported.
- [ ] Validate short WebSocket disconnect and reattach.
- [ ] Validate idle timeout.
- [ ] Validate absolute max duration.
- [ ] Validate per-user session limit.
- [ ] Validate per-node session limit.

## Agent Transport Validation

- [ ] Validate outbound TCP/443 transport.
- [ ] Validate agent reconnect with backoff.
- [ ] Validate control-plane detection of offline node.
- [ ] Validate terminal behavior during agent disconnect.
- [ ] Validate terminal behavior after agent reconnect.
- [ ] Validate revoked agent cannot reconnect.
- [ ] Validate QUIC fast path if implemented.

## UI Validation

- [ ] Validate iPhone 11 Pro: `375x812`.
- [ ] Validate iPad 11 Pro portrait: `834x1194`.
- [ ] Validate iPad 11 Pro landscape: `1194x834`.
- [ ] Validate laptop: `1366x768`.
- [ ] Validate desktop: `1920x1080`.
- [ ] Capture screenshots or test artifacts where practical.

## Release Documentation

- [ ] Update deployment runbook.
- [ ] Update backup and restore documentation.
- [ ] Update security documentation.
- [ ] Update agent transport documentation.
- [ ] Update terminal lifecycle documentation.
- [ ] Update known limitations.

---

# Bug / Issue Backlog

Add bugs here as they are discovered.

- [ ] Remote terminal reattach is not production-complete.
- [x] UI still needs production component/page split.
- [ ] Some production-critical state is still in memory.
- [ ] Agent-control-plane transport still needs production TCP/443 baseline.

---

# Decision Log

## 2026-05-08

- [x] The MVP is considered complete.
- [x] The next phase targets production readiness.
- [x] All documentation and code-facing text must be written in English.
- [x] Sunbolt will have only `development` and `production` runtime modes.
- [x] Sunbolt will own the agent-control-plane transport.
- [x] Agent nodes must use outbound connections and must not require inbound firewall access.
- [x] TCP/443 is the required production baseline for agent connectivity.
- [x] QUIC over UDP/443 is optional and cannot be the only production path.
- [x] Browser navigation must detach from terminal sessions, not kill PTYs.
- [x] Terminal UI must support multiple tabs/sessions.
- [x] UI must be responsive and adaptive across mobile, tablet, laptop, and desktop.
- [x] Required baseline viewports are iPhone 11 Pro, iPad 11 Pro portrait, iPad 11 Pro landscape, laptop, and desktop.

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
