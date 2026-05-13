# Backup and Restore

Sunbolt production state belongs in PostgreSQL. Backups must protect identity,
authorization, node enrollment, credential metadata, terminal session metadata,
and audit records.

## Backup Policy

Run PostgreSQL backups before every production upgrade and on a regular schedule
that matches the deployment recovery point objective. Keep backups encrypted at
rest, restrict access to trusted operators, and store copies outside the primary
database host.

Backups should include:

- Application tables and migration metadata.
- Audit event records and audit chain fields.
- Node records, credential fingerprints, expiration metadata, and revocation state.
- Terminal session metadata and lifecycle state.
- User, session, MFA, role, permission, workspace, and membership state.

Do not back up plaintext secrets into shared locations. If a platform secret
manager stores database credentials, cookie secrets, TLS keys, or node private
material, use that platform's backup procedure separately.

## Create a Backup

Confirm database connectivity:

```bash
psql "$SUNBOLT_DATABASE_URL" -c "select 1"
```

Create a custom-format backup:

```bash
pg_dump --format=custom --file=sunbolt-$(date +%Y%m%d%H%M%S).dump "$SUNBOLT_DATABASE_URL"
```

Record the application git commit, image digest, migration version, backup file
name, backup timestamp, and operator who created the backup.

## Restore Procedure

Restore into an empty database or a clearly isolated recovery database first.
Avoid restoring directly over a running production database unless the incident
response plan explicitly requires it.

```bash
pg_restore --clean --if-exists --dbname="$SUNBOLT_DATABASE_URL" sunbolt-YYYYMMDDHHMMSS.dump
```

After restore:

1. Run migrations required by the application version being started.
2. Start the control plane with `SUNBOLT_ENV=production`.
3. Verify `GET /health` returns HTTP 200.
4. Verify login, MFA where configured, and authorization checks.
5. Verify agent heartbeat and revocation state.
6. Verify terminal session metadata is readable and stale sessions are handled by policy.
7. Verify audit chain integrity before returning the instance to service.

## Restore Testing

Test restores periodically in an isolated environment. A restore test is not
complete until an operator can log in, inspect nodes, confirm audit records, and
open or intentionally reject a terminal according to policy.

