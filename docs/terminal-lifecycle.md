# Terminal Lifecycle

Terminal lifetime must be independent from browser page lifetime. A browser page is an attachment to a terminal session, not the owner of the PTY.

## Target State Model

The production lifecycle should model states equivalent to:

```text
Created -> Starting -> Active -> Detached -> Reattaching -> Active
Active -> Terminating -> Terminated
Active -> Failed
Detached -> Expired
```

The exact enum may differ, but the implementation must distinguish these events:

- UI tab closed.
- Browser WebSocket disconnected.
- User detached.
- User explicitly terminated the terminal.
- PTY process exited.
- Backend policy closed the terminal.
- Node disconnected.
- Node revoked.

## Open

Opening a terminal requires authentication, authorization, optional step-up MFA, audit logging, lifecycle tracking, and durable terminal metadata.

Production metadata should include:

- Terminal session ID.
- Actor and workspace.
- Node ID and route.
- Lifecycle state.
- Created, attached, detached, terminated, and expiry timestamps where applicable.
- Idle timeout and absolute max duration policy.
- Last output sequence number when replay is supported.

## Detach

Detach preserves the PTY and releases the browser attachment.

Detach happens when:

- The user explicitly detaches.
- The browser navigates away or refreshes.
- The browser WebSocket disconnects briefly.
- A UI terminal tab is closed without an explicit terminate request.

Detach should write an audit event and update durable metadata in production.

## Reattach

Reattach restores a browser attachment to an existing terminal session.

Reattach must verify:

- Authenticated browser session.
- User identity.
- Workspace membership.
- `terminal.reattach` or equivalent permission.
- Terminal ownership or delegated access.
- Session state allows reattach.
- Node is still trusted and not revoked.

The UI may use a reconnect token as an implementation detail, but production authorization must not depend only on a bearer reconnect token.

## Terminate

Terminate closes the PTY. It is distinct from closing a UI tab.

Termination may be caused by:

- Explicit user terminate action.
- Admin action.
- Idle timeout.
- Absolute max duration.
- Policy enforcement.
- Node revocation.
- PTY process exit.
- Non-recoverable terminal or transport failure.

Terminate should write an audit event with the reason and actor when available.

## Output Replay

Terminal output should carry sequence numbers when practical. A short replay buffer can help recover from short browser disconnects without losing the most recent output.

Replay buffers are runtime aids, not a replacement for durable terminal metadata or future session recording.

## Session Limits

Production should enforce:

- Maximum terminal sessions globally.
- Maximum sessions per user.
- Maximum sessions per node.
- Idle timeout.
- Absolute max duration.

Limits must apply to detached sessions as well as actively attached sessions.

## Release Validation

Before a production release, validate terminal behavior in the target
environment:

- Opening a terminal requires authentication, authorization, optional step-up MFA, audit logging, lifecycle tracking, and durable metadata.
- Input, output, resize, detach, reattach where supported, close UI tab, and explicit terminate actions behave distinctly.
- Browser route changes, refreshes, and short WebSocket disconnects detach instead of killing the PTY where recovery is supported.
- Reattach verifies user identity, workspace membership, permission, session ownership or delegated access, allowed lifecycle state, and node trust state.
- Revoked nodes cannot open new terminals and cannot continue active terminal sessions.
- Idle timeout, absolute max duration, per-user limits, and per-node limits apply to detached and active sessions.
- Terminal output sequence numbers and replay buffers recover recent output where implemented.
- Audit events are written for open, detach, reattach, terminate, close, failure, timeout, and revocation-driven closure.
