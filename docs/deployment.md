# Sunbolt Deployment

These notes describe the production deployment direction for Sunbolt. A production deployment runs the control plane behind a TLS-terminating reverse proxy, uses PostgreSQL for durable state, and accepts outbound-only agent connections from managed nodes.

Target production flow:

```text
Browser -> HTTPS/WebSocket -> Control Plane -> Sunbolt Native Outbound Agent Channel -> Agent Node -> PTY
```

## Runtime Modes

Sunbolt has exactly two runtime modes:

- `development`
- `production`

`SUNBOLT_ENV` is required at startup and must be set to exactly one of these
two values. Missing, empty, or unknown runtime modes fail startup.

Use `development` for local operator work, temporary in-memory scaffolding, local bootstrap credentials, and permissive local origins.

Use `production` for any deployment that protects real systems or real user accounts. Production mode must reject development-only shortcuts.

Preview, staging, and test deployments should choose one of these two modes with environment-specific configuration. Do not add a third mode for deployment labels.

## Build and Run

Build the production image:

```bash
docker build -t sunbolt:latest .
```

Run the control plane with a production environment file:

```bash
docker run --rm --env-file config/production.env -p 3000:3000 sunbolt:latest
```

The image starts `sunbolt-control` by default and also includes the `sunbolt-agent` binary for managed-node deployments.

## Production Release Runbook

Use this runbook before promoting a build to production.

1. Confirm the release candidate is built from the intended git commit and that no uncommitted files are included in the operator workspace.
2. Review configuration against `config/production.env.example` and confirm `SUNBOLT_ENV=production`.
3. Confirm PostgreSQL connectivity from the deployment environment with `psql "$SUNBOLT_DATABASE_URL" -c "select 1"`.
4. Back up PostgreSQL before applying migrations or replacing a running release.
5. Run pending migrations from a trusted operator workstation or one-off job.
6. Start or roll the control plane behind the TLS-terminating reverse proxy.
7. Verify `GET /health` returns HTTP 200.
8. Verify browser HTTPS access, session cookie settings, WebSocket origin checks, and terminal WebSocket upgrade behavior.
9. Verify at least one enrolled agent can connect outbound through TCP/443, heartbeat, and reconnect.
10. Verify terminal open, input, output, resize, detach, reattach where supported, and terminate behavior.
11. Verify audit events are written for login, MFA where applicable, terminal lifecycle, agent connection, transport negotiation, and node revocation actions.
12. Review structured logs for request IDs, actor IDs or emails, node IDs, session IDs, and transport IDs.

Do not promote the build if production startup validation fails, audit writes fail, PostgreSQL is unavailable, secure cookie configuration is unsafe, wildcard browser origins are configured, or development bootstrap admin is enabled.

## Production Configuration

Start from `config/production.env.example` and set deployment-specific values:

- `SUNBOLT_ENV=production`.
- `SUNBOLT_DATABASE_URL` points at the production PostgreSQL database.
- `SUNBOLT_PUBLIC_URL` matches the HTTPS origin users access.
- `SUNBOLT_ALLOWED_ORIGINS` lists explicit HTTPS browser origins. Do not use `*`.
- `SUNBOLT_COOKIE_SECURE=true`.
- `SUNBOLT_DEV_BOOTSTRAP_ADMIN=false`.
- `SUNBOLT_REQUIRE_TERMINAL_STEP_UP_MFA=true` unless an approved policy says otherwise.
- Terminal session limits and timeouts are sized for the deployment.

Production startup fails clearly when required durable storage, `SUNBOLT_DATABASE_URL`, `SUNBOLT_PUBLIC_URL`, secure cookies, explicit browser origins, or development bootstrap settings are missing or unsafe.

Keep production secrets out of git. Inject them through the platform secret manager or an environment file with restricted filesystem permissions.

## Reverse Proxy and HTTPS

Use `deploy/nginx/sunbolt.conf` as a starting point. The reverse proxy must:

- Redirect HTTP to HTTPS for browser traffic.
- Terminate TLS with a valid certificate.
- Forward `Host`, `X-Forwarded-For`, and `X-Forwarded-Proto`.
- Preserve WebSocket upgrade headers for `/terminal/ws`.
- Use long enough proxy read/send timeouts for active terminal sessions.
- Route the production agent transport endpoint over TLS/TCP/443 when implemented.

Do not expose browser terminal routes over plain HTTP in production. The public edge for browser terminal access must use TLS.

## Agent Connectivity

Agent nodes must not require inbound firewall access. Agents initiate outbound connections to the control plane.

Production transport priorities:

1. Baseline: TLS over TCP/443 using WebSocket or HTTP/2.
2. Optional fast path: QUIC over UDP/443 when the network allows it.
3. Restrictive-network fallback: long-poll or request/response control channel if required.

QUIC must not be the only production path because many enterprise networks block UDP.

When QUIC is enabled, expose UDP/443 at the edge and keep TCP/443 available for
the baseline WebSocket or HTTP/2 transport. The agent should try the QUIC
candidate endpoint first, then fall back to TCP/443, then use long-poll HTTPS
only when streaming transports are unavailable.

## Database Migrations

Run migrations before starting a new application version:

```bash
sea-orm-cli migrate up -d migrations -u "$SUNBOLT_DATABASE_URL"
```

For a fresh database, verify connectivity first:

```bash
psql "$SUNBOLT_DATABASE_URL" -c "select 1"
```

Run migrations from a trusted operator workstation or one-off deployment job, not from every application container at startup.

## Health Checks

The control plane exposes:

```text
GET /health
```

A healthy process returns HTTP 200 with a small JSON body.

Health checks confirm process availability. Production readiness also requires database connectivity, migration state, agent transport health, and audit log write availability.

## Backup and Restore

Back up PostgreSQL before upgrades and on a regular schedule. See
[Backup and Restore](backup-restore.md) for the full operator procedure.

Create a custom-format backup:

```bash
pg_dump --format=custom --file=sunbolt-$(date +%Y%m%d%H%M%S).dump "$SUNBOLT_DATABASE_URL"
```

Restore into an empty database:

```bash
pg_restore --clean --if-exists --dbname="$SUNBOLT_DATABASE_URL" sunbolt-YYYYMMDDHHMMSS.dump
```

After restore, run audit chain verification before returning the instance to service. Keep backup files encrypted at rest, restrict operator access, and test restores periodically.

## Release Gate

Before treating a build as production-ready, complete the automated and manual
release gate:

- Run `cargo fmt --all -- --check`.
- Run `cargo test`.
- Run `cargo clippy --all-targets --all-features -- -D warnings`.
- Run database migration verification when migrations changed.
- Validate terminal open, detach, reattach, resize, and terminate behavior.
- Validate agent enrollment, heartbeat, reconnect, and revocation behavior.
- Validate UI behavior on mobile, tablet, laptop, and desktop viewports.
- Review [Security Model](security-model.md), [Agent Transport](agent-transport.md), [Terminal Lifecycle](terminal-lifecycle.md), and [Known Limitations](known-limitations.md) for release-specific risks.
