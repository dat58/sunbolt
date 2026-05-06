# syntax=docker/dockerfile:1

FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations

RUN cargo build --locked --release -p sunbolt-control -p sunbolt-agent

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /var/lib/sunbolt --shell /usr/sbin/nologin sunbolt

COPY --from=builder /app/target/release/sunbolt-control /usr/local/bin/sunbolt-control
COPY --from=builder /app/target/release/sunbolt-agent /usr/local/bin/sunbolt-agent

USER sunbolt
ENV SUNBOLT_ENV=production \
    SUNBOLT_BIND_ADDR=0.0.0.0:3000 \
    SUNBOLT_COOKIE_SECURE=true \
    SUNBOLT_DEV_BOOTSTRAP_ADMIN=false
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/sunbolt-control"]
