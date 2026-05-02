# AGENTS.md

## Project: Sunbolt

Sunbolt is a web-based remote terminal and distributed server control platform. It allows users to open secure browser-based terminals from laptops, desktops, phones, and other devices to interact with servers managed by the platform.

The long-term goal is to evolve from a single-server web terminal into an agent-based distributed system with centralized management, secure node communication, MFA, RBAC, audit logs, and eventually mesh-style node connectivity.

## Core Principles

1. Security first.
2. Auditability by default.
3. Rust-first implementation.
4. Small, testable, incremental changes.
5. Prefer explicit architecture over framework magic.
6. Keep the MVP simple, but design module boundaries so distributed features can be added later.

## Technology Direction

Use the following stack unless there is a strong reason to change it:

- Language: Rust
- UI: Dioxus Web
- Backend API: Axum
- Async runtime: Tokio
- Database: PostgreSQL
- ORM/migrations: SeaORM / SeaORM migration
- Terminal transport: WebSocket
- Terminal rendering in browser: xterm.js or equivalent JS terminal emulator integrated into Dioxus
- Agent/node runtime: Rust daemon
- Serialization: serde first; consider bincode/postcard/protobuf later
- Auth/session: secure server-side sessions or secure HttpOnly cookies
- MFA design: abstract factor pipeline with support for WebAuthn/passkeys, TOTP, recovery codes, and future factors

## Architecture Rules

### Dioxus Usage

Dioxus should primarily own the web UI.

Do not hide critical backend behavior inside Dioxus server functions if it makes terminal/session/auth logic harder to test or reason about. Prefer an explicit Axum backend for:

- Authentication
- Authorization
- WebSocket terminal sessions
- Agent communication
- Audit logging
- Node management
- Database access

Dioxus fullstack features may be used selectively, but they must not become the core boundary for terminal, security, or distributed-system logic.

### Backend Boundaries

Prefer a Rust workspace with separate crates or modules for:

- `sunbolt-ui`
- `sunbolt-control`
- `sunbolt-agent`
- `sunbolt-auth`
- `sunbolt-terminal`
- `sunbolt-protocol`
- `sunbolt-audit`
- `sunbolt-storage`
- `sunbolt-common`

The exact crate structure may evolve, but the separation of responsibilities should remain clear.

### Terminal Rules

Remote terminals must be treated as high-risk functionality.

Every terminal open request must pass through:

1. Authentication check
2. Authorization check
3. Optional step-up MFA policy check
4. Audit log write
5. Session lifecycle tracking

Terminal sessions should support, or be designed to later support:

- Input stream
- Output stream
- Resize events
- Close events
- Reconnect/detach
- Idle timeout
- Max session limit per user
- Max session limit per node
- Optional session recording

### Distributed System Rules

Start with a centralized control plane and local terminal support.

Then add agent nodes.

Do not implement pure P2P first. The target direction is:

```text
Browser -> Control Plane -> Agent Node -> Local PTY
```

Later phases may add direct tunnels, relay nodes, or mesh routing, but the central control plane remains the source of truth for:

- Identity
- Authentication
- Authorization
- Node enrollment
- Audit logs
- Policy

### Node Security Rules

Node enrollment must be designed around a one-time enrollment token.

Long-term node trust should use node identity and certificate/key material, not permanent shared passwords.

Future node communication should support:

- mTLS
- Certificate rotation
- Node revocation
- Agent version tracking
- Heartbeat and health status

### Auth/MFA Rules

Do not hard-code authentication as only username/password.

Design MFA as an extensible factor system.

Examples of supported or planned factors:

- Password
- WebAuthn/passkey
- TOTP
- Recovery code
- Email OTP
- Hardware key
- Admin approval
- SSH key signature

Opening a production terminal should be compatible with step-up MFA.

### Authorization Rules

Do not rely on UI-level button hiding for security.

Every backend route and WebSocket command must verify permissions server-side.

Use resource-oriented permissions such as:

- `node.view`
- `node.register`
- `node.revoke`
- `terminal.open`
- `terminal.close`
- `terminal.view_history`
- `terminal.recording.view`
- `audit.view`
- `user.manage`
- `role.manage`

Prefer a workspace/project model for grouping nodes and users.

## UI/UX Direction

The product name is **Sunbolt**.

The visual identity should combine:

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

## Testing Rules

Every meaningful code change must include or update tests when practical.

At minimum, before committing any change, run:

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

If the workspace later adds formatting rules, also run:

```bash
cargo fmt --all -- --check
```

If frontend tooling is added, run the relevant frontend checks as documented by the project.

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

Update documentation when behavior or architecture changes.

Relevant docs include:

- `README.md`
- `PLAN.md`
- `TASK.md`
- API/protocol docs
- migration notes
- security notes

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

Development-only shortcuts must be clearly marked and isolated behind local/dev configuration.
