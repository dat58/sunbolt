# Local Development

This guide runs the Sunbolt control plane, Dioxus web UI, and local agent while production hardening is in progress.

Local development uses the `development` runtime mode. Development mode may include bootstrap credentials, permissive local origins, and in-memory scaffolding. Those shortcuts are not production behavior and must remain disabled in `production`.

## Quick Start With Make

From the repository root:

```bash
make env-init
make control
make ui
make agent
```

Useful supporting targets:

- `make env-init`: create `.env` from `.env.example` when missing.
- `make ui-css-watch`: keep the Tailwind bundle updating during UI work.
- `make agent-token`: print a fresh one-time enrollment token without starting the agent.
- `make checks`: run `cargo test` and `cargo clippy --all-targets --all-features -- -D warnings`.
- `make release-checks`: run the full local release gate, including `cargo fmt --all -- --check`.

The `Makefile` auto-loads `.env` when present. Override ports or local credentials per command when needed:

```bash
make control SUNBOLT_ENV=development ALLOWED_ORIGINS=http://127.0.0.1:8080
make ui UI_PORT=8081
make agent AGENT_NODE_NAME=$(hostname)
```

## Runtime Mode Boundary

Development mode exists for local iteration. It may allow:

- Local bootstrap admin credentials controlled by `SUNBOLT_DEV_BOOTSTRAP_ADMIN`.
- Local HTTP origins such as `http://127.0.0.1:8080`.
- The `SUNBOLT_ALLOWED_ORIGINS=*` browser-origin shortcut.
- In-memory terminal, auth, node, or audit scaffolding while a feature is being moved to durable storage.
- Local PTYs on the same host as `sunbolt-control`.

Production mode must not allow:

- Hidden or default admin credentials.
- Wildcard browser origins.
- Auth tokens in `localStorage`.
- Plaintext node credentials.
- Production source-of-truth state stored only in process memory.
- Public terminal routes without TLS at the edge.

Preview and test deployments should run as either `development` or `production` with separate configuration and secrets. Do not introduce a third runtime mode for preview, staging, or test.

## Control Plane

Start from the repository root:

```bash
cp .env.example .env
set -a
. ./.env
set +a
SUNBOLT_ENV=development cargo run -p sunbolt-control
```

The control plane binds to `SUNBOLT_BIND_ADDR`, which defaults to `127.0.0.1:3000`.

Useful local endpoints:

- Health: `http://127.0.0.1:3000/health`
- Terminal WebSocket: `ws://127.0.0.1:3000/terminal/ws`
- Step-up MFA: `http://127.0.0.1:3000/auth/mfa/step-up`

## Dioxus UI

Build the generated CSS asset before serving the UI:

```bash
cd crates/sunbolt-ui
npm run css:build
```

Run the Dioxus web UI with the Dioxus CLI:

```bash
SUNBOLT_CONTROL_PLANE_URL=http://127.0.0.1:3000 \
dx serve --platform web --package sunbolt-ui --port 8080
```

When the UI is served from a different origin than the control plane, allow that origin in the backend. For local development only, this permissive shortcut is available:

```bash
SUNBOLT_ENV=development SUNBOLT_ALLOWED_ORIGINS=* cargo run -p sunbolt-control
```

For a tighter local setup, allow only the Dioxus origin:

```bash
SUNBOLT_ENV=development SUNBOLT_ALLOWED_ORIGINS=http://127.0.0.1:8080 cargo run -p sunbolt-control
```

`SUNBOLT_ALLOWED_ORIGINS=*` is a development shortcut. The backend reflects the browser `Origin` header on credentialed responses instead of sending a literal `Access-Control-Allow-Origin: *`, because login and session APIs rely on cookies. Production configuration must list explicit HTTPS origins.

## Local Agent

To enroll and heartbeat a local `sunbolt-agent` process against the control plane, follow [Agent Connection](agent-connection.md).

The current development agent flow uses one-time enrollment tokens and a local heartbeat API. The production target is a Sunbolt-native outbound agent channel over TLS/TCP/443 with durable node identity, credential rotation, revocation, heartbeats, reconnect, backpressure, and transport metrics.

## Environment Variables

Common local control-plane variables:

- `SUNBOLT_ENV`: runtime mode. Use `development` for local work and `production` for production deployments.
- `SUNBOLT_BIND_ADDR`: HTTP bind address, default `127.0.0.1:3000`.
- `SUNBOLT_ALLOWED_ORIGINS`: comma-separated browser origins allowed for state-changing routes and WebSocket origin checks.
- `SUNBOLT_DEV_BOOTSTRAP_ADMIN`: enables the development bootstrap admin when `true`.
- `SUNBOLT_DEV_ADMIN_EMAIL`: bootstrap admin email, default `admin@sunbolt.local`.
- `SUNBOLT_DEV_ADMIN_PASSWORD`: bootstrap admin password, default `sunbolt-dev-admin`.
- `SUNBOLT_REQUIRE_TERMINAL_STEP_UP_MFA`: requires recent MFA before terminal open when `true`.
- `SUNBOLT_TERMINAL_IDLE_TIMEOUT_SECS`: terminal idle timeout.
- `SUNBOLT_TERMINAL_DISCONNECT_GRACE_SECS`: detach grace window for short browser disconnects.
- `SUNBOLT_MAX_TERMINAL_SESSIONS`: global terminal session limit.
- `SUNBOLT_MAX_TERMINAL_SESSIONS_PER_USER`: per-user terminal session limit.
- `SUNBOLT_MAX_TERMINAL_SESSIONS_PER_NODE`: per-node terminal session limit.

Storage variables are listed in `.env.example`. PostgreSQL is mandatory for production, even while some local development paths still use in-memory scaffolding.

## Development Limitations

- Full browser session reattach UI is still minimal.
- Remote terminal reattach is not production-complete.
- Some production-critical state still needs PostgreSQL-backed repositories.
- The production agent transport baseline still needs target-environment release validation for outbound TCP/443, reconnect, and degraded fallback behavior.
- Mobile, tablet, laptop, and desktop layout contracts have static `cargo test`
  coverage for the required viewport sizes. Browser screenshot validation is
  still manual until end-to-end UI automation is added.
- See [Known Limitations](known-limitations.md) for the release gate risk list.
