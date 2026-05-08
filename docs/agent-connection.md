# Agent Connection

This guide connects a local `sunbolt-agent` process to a running Sunbolt control plane.

The current development flow uses:

- Control plane: `http://127.0.0.1:3000`
- Agent enrollment endpoint: `POST /agent/enroll`
- Agent heartbeat endpoint: `POST /agent/heartbeat`
- Node API: `GET /nodes`

Node enrollment uses a one-time token created by an authenticated admin or operator.

## Start the Control Plane

From the repository root:

```bash
set -a
. ./.env
set +a
SUNBOLT_ALLOWED_ORIGINS=http://127.0.0.1:8080 cargo run -p sunbolt-control
```

The local control plane should respond at:

```bash
curl http://127.0.0.1:3000/health
```

## Create an Enrollment Token

In another terminal, authenticate and create a one-time enrollment token:

```bash
COOKIE_JAR="$(mktemp)"

curl -sS \
  -c "$COOKIE_JAR" \
  -H 'content-type: application/json' \
  -X POST http://127.0.0.1:3000/auth/login \
  -d '{"email":"admin@sunbolt.local","password":"sunbolt-dev-admin"}'

TOKEN="$(
  curl -sS \
    -b "$COOKIE_JAR" \
    -H 'content-type: application/json' \
    -X POST http://127.0.0.1:3000/nodes/enrollment-tokens \
    -d '{"expires_in_secs":900}' \
  | jq -r '.token'
)"

echo "$TOKEN"
```

If your local `.env` overrides the bootstrap admin, use the configured `SUNBOLT_DEV_ADMIN_EMAIL` and `SUNBOLT_DEV_ADMIN_PASSWORD` instead.

## Start the Agent

Use the token once. A successful enrollment consumes it.

```bash
SUNBOLT_CONTROL_PLANE_URL=http://127.0.0.1:3000 \
SUNBOLT_AGENT_NODE_NAME="$(hostname)" \
SUNBOLT_AGENT_ENROLLMENT_TOKEN="$TOKEN" \
SUNBOLT_AGENT_IDENTITY_PATH="$HOME/.config/sunbolt/identity.json" \
cargo run -p sunbolt-agent
```

Expected agent logs include:

```text
agent enrolled with control plane
agent heartbeat accepted
```

The agent sends a heartbeat every 30 seconds until the process is stopped.

After the first successful enrollment, the agent persists node identity material
to `SUNBOLT_AGENT_IDENTITY_PATH`. Later starts should use the same identity file
and do not need `SUNBOLT_AGENT_ENROLLMENT_TOKEN` unless the node is being
enrolled again.

The identity file contains the node ID, credential secret, credential
fingerprint, credential expiration timestamp, and agent version. On Unix hosts,
Sunbolt creates the parent directory with mode `0700` and the identity file with
mode `0600`. Operators should apply equivalent owner-only read/write access on
other platforms and should not copy this file between nodes.

## Verify the Node

Use the same authenticated cookie jar:

```bash
curl -sS -b "$COOKIE_JAR" http://127.0.0.1:3000/nodes | jq .
```

The enrolled node should show `status: "online"` shortly after the first heartbeat.

## Notes

- Enrollment tokens are one-time credentials. Create a new token for each agent run that needs a fresh enrollment.
- The control plane returns node credential material only during enrollment and stores the credential fingerprint plus expiration metadata.
- Stop the agent with `Ctrl-C`.
- The Dioxus Nodes page is still mostly placeholder UI; use the `/nodes` API response to verify real node state during development.
