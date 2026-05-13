# Known Limitations

This document lists release risks that must be reviewed before treating a build
as production-ready.

## Terminal Recovery

Remote terminal reattach is not production-complete. Local terminal lifecycle
tests cover detach, cleanup, timeout, session limits, and explicit termination,
but remote recovery after agent disconnect and reconnect still requires manual
validation and additional production hardening.

Browser route changes and refresh recovery must be validated against the target
deployment before production use. Where recovery is not supported, the UI and
operator runbook must describe the failure mode clearly.

## Agent Transport

The Sunbolt-native baseline transport is designed around outbound TCP/443 using
WebSocket or HTTP/2. The long-poll HTTPS fallback is a degraded reachability path
and should be used only when streaming transports are unavailable.

QUIC over UDP/443 is optional. A deployment must keep the TCP/443 baseline
available even when QUIC is enabled because many enterprise networks block UDP.

## Durable State

Production-critical source-of-truth state must live in PostgreSQL. In-memory
state may still hold live sockets, PTY handles, replay buffers, and runtime
transport handles. Operators should not rely on in-memory state for recovery
after process restart.

## UI Validation

Static viewport validation is recorded through `cargo test -p sunbolt-ui
viewport_validation`. Browser screenshot validation remains a manual release
activity until end-to-end UI automation is added.

## Release Gate

A build is not production-ready until `cargo fmt --all -- --check`,
`cargo test`, and `cargo clippy --all-targets --all-features -- -D warnings`
pass, changed migrations are verified, and the deployment runbook has been
completed for the target environment.

