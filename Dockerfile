# syntax=docker/dockerfile:1

FROM rust:1-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler pkg-config libssl-dev curl \
    && rm -rf /var/lib/apt/lists/*

ARG TRUNK_VERSION=0.21.14
RUN rustup target add wasm32-unknown-unknown \
    && curl -fsSL https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz \
       | tar xz -C /usr/local/cargo/bin

COPY Cargo.toml build.rs schema.sql ./
COPY proto ./proto
COPY src ./src
COPY config ./config
COPY frontend ./frontend
COPY static ./static

# Skip frontend build if dist was pre-built (CI passes the artifact).
RUN if [ ! -f frontend/dist/index.html ]; then \
      cd frontend && trunk build --release; \
    fi

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp target/release/denpie /app/denpie-binary

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home /var/lib/denpie --create-home --shell /usr/sbin/nologin denpie

WORKDIR /app
COPY --from=builder /app/denpie-binary /usr/local/bin/denpie
COPY schema.sql /app/schema.sql
COPY --from=builder /app/frontend/dist /app/frontend/dist
COPY static /app/static

ENV DENPIE_BIND_ADDR=127.0.0.1:3017 \
    DENPIE_DATA_DIR=/var/lib/denpie \
    DENPIE_SCHEMA_PATH=/app/schema.sql \
    DENPIE_FRONTEND_DIST=/app/frontend/dist \
    DENPIE_STATIC_DIR=/app/static

VOLUME ["/var/lib/denpie"]
EXPOSE 3017
RUN chmod -R a+rX /app/frontend/dist /app/static
USER denpie

CMD ["denpie"]
