# Local Development

This guide runs the Sunbolt control plane and Dioxus web UI for the local terminal MVP.

## Quick Start With Make

From the repository root you can now use:

```bash
make control
make ui
make agent
```

Useful supporting targets:

- `make env-init`: create `.env` from `.env.example` when missing.
- `make ui-css-watch`: keep the Tailwind bundle updating during UI work.
- `make agent-token`: print a fresh one-time enrollment token without starting the agent.
- `make checks`: run `cargo test` and `cargo clippy --all-targets --all-features -- -D warnings`.

The `Makefile` auto-loads `.env` when present. Override ports or local credentials per command when needed, for example:

```bash
make control ALLOWED_ORIGINS=http://127.0.0.1:8080
make ui UI_PORT=8081
make agent AGENT_NODE_NAME=$(hostname)
```

## Control Plane

Start from the repository root:

```bash
cp .env.example .env
set -a
. ./.env
set +a
cargo run -p sunbolt-control
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

When the UI is served from a different origin than the control plane, allow that origin in the backend. For local development you can use the permissive shortcut below:

```bash
SUNBOLT_ALLOWED_ORIGINS=* cargo run -p sunbolt-control
```

For a tighter local setup, allow only the Dioxus origin instead:

```bash
SUNBOLT_ALLOWED_ORIGINS=http://127.0.0.1:8080 cargo run -p sunbolt-control
```

The UI now uses `SUNBOLT_CONTROL_PLANE_URL` to derive `auth` and terminal endpoints. It injects `window.SUNBOLT_CONTROL_PLANE_URL` before the terminal bridge runs, so the browser talks to the control plane on `127.0.0.1:3000` even when Dioxus serves the UI from `127.0.0.1:8080`.

`SUNBOLT_ALLOWED_ORIGINS=*` is a local-dev shorthand. The backend reflects the browser `Origin` header on credentialed responses instead of sending a literal `Access-Control-Allow-Origin: *`, because login and session APIs rely on cookies.

## Local Agent

To enroll and heartbeat a local `sunbolt-agent` process against the control plane, follow [Agent Connection](agent-connection.md).

## Environment Variables

Common local control-plane variables:

- `SUNBOLT_BIND_ADDR`: HTTP bind address, default `127.0.0.1:3000`.
- `SUNBOLT_ALLOWED_ORIGINS`: comma-separated browser origins allowed for state-changing routes and WebSocket origin checks.
- `SUNBOLT_DEV_BOOTSTRAP_ADMIN`: enables the local bootstrap admin when `true`, default `true`.
- `SUNBOLT_DEV_ADMIN_EMAIL`: bootstrap admin email, default `admin@sunbolt.local`.
- `SUNBOLT_DEV_ADMIN_PASSWORD`: bootstrap admin password, default `sunbolt-dev-password`.
- `SUNBOLT_REQUIRE_TERMINAL_STEP_UP_MFA`: requires recent MFA before terminal open when `true`, default `true`.
- `SUNBOLT_TERMINAL_IDLE_TIMEOUT_SECS`: terminal idle timeout.
- `SUNBOLT_TERMINAL_DISCONNECT_GRACE_SECS`: detach grace window for short browser disconnects.
- `SUNBOLT_MAX_TERMINAL_SESSIONS`: global terminal session limit.
- `SUNBOLT_MAX_TERMINAL_SESSIONS_PER_USER`: per-user terminal session limit.
- `SUNBOLT_MAX_TERMINAL_SESSIONS_PER_NODE`: per-node terminal session limit.

Storage variables are listed in `.env.example`; the current in-memory MVP paths do not require PostgreSQL for opening a local terminal.

## MVP Limitations

- The UI uses xterm.js from a CDN during local development.
- Full browser session reattach UI is still minimal; the reconnect button sends the protocol reattach message when a reconnect token is available.
- Local terminal sessions run on the host where `sunbolt-control` is running unless a node id routes to an enrolled agent.
- Mobile browser validation is still manual.
