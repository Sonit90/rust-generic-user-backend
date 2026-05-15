# syntax=docker/dockerfile:1.7
# Multi-stage build for the price-merger api binary.

# ---------- builder ----------
FROM rust:1.95-bookworm AS builder
WORKDIR /build

# Pre-cache deps using a dummy main, then build for real.
COPY Cargo.toml Cargo.lock* ./
COPY crates ./crates
COPY migrations ./migrations
COPY config ./config

# Make sure SQLx offline mode is honored if .sqlx is present.
ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --locked --bin price-merger-api && \
    cp /build/target/release/price-merger-api /usr/local/bin/price-merger-api

# ---------- runtime ----------
FROM debian:bookworm-slim AS runtime
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates tini && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --system app && useradd --system --gid app --create-home app

WORKDIR /app
COPY --from=builder /usr/local/bin/price-merger-api /usr/local/bin/price-merger-api
COPY config ./config
COPY migrations ./migrations

USER app
EXPOSE 8080
ENV APP_ENV=production \
    RUST_LOG=info,price_merger=info,sqlx=warn

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/price-merger-api"]
