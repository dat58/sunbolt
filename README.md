# Sunbolt

Sunbolt is a Rust-first web-based remote terminal and distributed server control platform.

The project starts with a local browser terminal MVP and is structured so it can grow into a centralized control plane with managed agent nodes, authentication, authorization, MFA, audit logs, and secure node communication.

## Workspace

The initial workspace separates core areas of responsibility:

- `sunbolt-ui`: web UI shell
- `sunbolt-control`: control plane
- `sunbolt-agent`: managed node agent
- `sunbolt-common`: shared types and helpers
- `sunbolt-auth`: authentication and authorization foundation
- `sunbolt-terminal`: terminal core boundary
- `sunbolt-protocol`: node protocol boundary
- `sunbolt-audit`: audit logging boundary
- `sunbolt-storage`: storage boundary

## Local Checks

Run these before committing code changes:

```bash
cargo fmt
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## Local Development

Control-plane and Dioxus UI startup instructions live in [docs/local-development.md](docs/local-development.md).

## Deployment

Production deployment notes, container build instructions, reverse proxy guidance, migration commands, and backup/restore notes live in [docs/deployment.md](docs/deployment.md).
