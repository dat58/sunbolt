# Local Development

This guide runs the Sunbolt control plane and Dioxus web UI for the local terminal MVP.

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
dx serve --platform web --package sunbolt-ui --port 8080
```

When the UI is served from a different origin than the control plane, allow that origin in the backend:

```bash
SUNBOLT_ALLOWED_ORIGINS=http://127.0.0.1:8080 cargo run -p sunbolt-control
```

The terminal bridge defaults to the current browser host plus `/terminal/ws`. If the UI is served separately from the control plane, configure the browser global before the Sunbolt app loads:

```html
<script>
  window.SUNBOLT_TERMINAL_WS_URL = "ws://127.0.0.1:3000/terminal/ws";
</script>
```

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
