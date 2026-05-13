# Security Model

Sunbolt provides browser access to real server terminals. The security model starts from the assumption that terminal access is high risk and every operation must be authorized server-side.

## Trust Boundaries

Primary trust boundaries:

- Browser to control plane over HTTPS and WebSocket.
- Control plane to PostgreSQL over a trusted private network or equivalent secure path.
- Agent node to control plane over a Sunbolt-owned outbound transport.
- Agent process to local PTY on the managed node.

The browser is not trusted to enforce authorization. UI visibility may improve usability, but backend HTTP routes and WebSocket commands must verify permissions independently.

## Runtime Modes

Sunbolt has exactly two runtime modes:

- `development`
- `production`

The control plane requires `SUNBOLT_ENV` at startup and accepts only
`development` or `production`.

Development mode may enable local bootstrap accounts, permissive local origins, and in-memory scaffolding.

Production mode must not enable hidden default credentials, wildcard browser origins, plaintext node credentials, auth tokens in `localStorage`, or production source-of-truth state that only exists in memory.
Production startup fails if required production configuration is missing, the
development bootstrap admin is enabled, wildcard or empty browser origins are
configured, or secure cookies are disabled.

## Authentication and Sessions

Production authentication must use secure server-side sessions or secure HttpOnly cookies. Browser scripts must not store auth tokens in `localStorage`.

Production session cookies should be:

- `HttpOnly`
- `Secure`
- `SameSite` according to the deployed browser flow
- Scoped to the production domain
- Rotated or invalidated on logout and security-sensitive changes

## Authorization

Authorization uses resource-oriented permissions. Examples include:

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

Every terminal open, reattach, input, resize, detach, close, and terminate operation should be associated with an authenticated actor and verified against workspace membership, node trust state, terminal ownership or delegated access, and the relevant permission.

## MFA and Step-Up Policy

Sunbolt must not hard-code authentication as username/password only. MFA is an extensible factor pipeline. Supported or planned factors include:

- Password
- WebAuthn/passkey
- TOTP
- Recovery code
- Email OTP
- Hardware key
- Admin approval
- SSH key signature

Opening a production terminal must be compatible with step-up MFA. Recent MFA state must be durable or recoverable in production.

## Node Identity

Node enrollment starts with a one-time enrollment token. Long-term node trust must use node identity material, not permanent shared passwords.

During enrollment, the control plane consumes the one-time token, creates the
stable node ID, generates node credential material, stores only the credential
fingerprint and expiration metadata, and returns the credential secret once to
the agent. The agent persists that identity material on the node host and reuses
it for later heartbeats and transport negotiation. For the current shared-secret
identity model, the agent sends a derived credential proof instead of the raw
secret; unknown nodes, expired credentials, invalid proofs, and revoked nodes
are rejected.

Production node identity should support:

- Node private key or certificate material.
- Credential fingerprint.
- Credential expiration.
- Credential rotation.
- Node revocation.
- Agent version tracking.
- Heartbeat and health state.

Agent identity files must be readable and writable only by the agent service
account. On Unix hosts, the directory should be `0700` and the file should be
`0600`; on other platforms, apply equivalent owner-only access controls.

Revoked nodes must be unable to open or continue terminal sessions.

## Terminal Safety

Every terminal open request must pass through:

1. Authentication check.
2. Authorization check.
3. Optional step-up MFA policy check.
4. Audit log write.
5. Session lifecycle tracking.
6. Durable terminal metadata write.

Browser navigation, page refresh, tab switch, route change, and short WebSocket disconnects detach from a terminal session. They must not kill the PTY. Only explicit terminate actions, backend timeout, policy enforcement, node revocation, process exit, or administrative action may close the PTY.

## Secret Handling

Do not log secrets. Redact tokens, credentials, recovery codes, passkey material, cookies, and opaque long-lived identifiers.

Production secrets belong in a platform secret manager or restricted environment file. Do not commit production secrets.
