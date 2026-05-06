# Sunbolt Deployment Notes

These notes cover the current single control-plane deployment shape. Sunbolt should be placed behind a TLS-terminating reverse proxy, with PostgreSQL reachable only from trusted application hosts.

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

## Configuration

Start from `config/production.env.example` and set deployment-specific values:

- `SUNBOLT_DATABASE_URL` must point at the production PostgreSQL database.
- `SUNBOLT_PUBLIC_URL` and `SUNBOLT_ALLOWED_ORIGINS` must match the HTTPS origin users access.
- `SUNBOLT_COOKIE_SECURE=true` is required when serving through HTTPS.
- `SUNBOLT_DEV_BOOTSTRAP_ADMIN=false` prevents development bootstrap credentials in production.
- Terminal session limits and timeouts should be sized for the deployment and reviewed before enabling broad access.

Keep production secrets out of git and inject them through the platform secret manager or an environment file with restricted filesystem permissions.

## Reverse Proxy and HTTPS

Use `deploy/nginx/sunbolt.conf` as a starting point. The reverse proxy must:

- Redirect HTTP to HTTPS.
- Terminate TLS with a valid certificate.
- Forward `Host`, `X-Forwarded-For`, and `X-Forwarded-Proto`.
- Preserve WebSocket upgrade headers for `/terminal/ws`.
- Use long enough proxy read/send timeouts for active terminal sessions.

Do not expose browser terminal routes over plain HTTP in production.

## Health Checks

The control plane exposes:

```text
GET /health
```

A healthy process returns HTTP 200 with a small JSON body.

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

## Backup and Restore

Back up PostgreSQL before upgrades and on a regular schedule:

```bash
pg_dump --format=custom --file=sunbolt-$(date +%Y%m%d%H%M%S).dump "$SUNBOLT_DATABASE_URL"
```

Restore into an empty database:

```bash
pg_restore --clean --if-exists --dbname="$SUNBOLT_DATABASE_URL" sunbolt-YYYYMMDDHHMMSS.dump
```

After restore, run the audit chain verification tooling before returning the instance to service. Keep backup files encrypted at rest, restrict operator access, and test restores periodically.
