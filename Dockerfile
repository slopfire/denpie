FROM rust:1-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown \
    && cargo install trunk --locked

COPY Cargo.toml build.rs schema.sql ./
COPY proto ./proto
COPY src ./src
COPY frontend ./frontend
COPY static ./static
RUN cd frontend && trunk build --release
RUN cargo build --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home /var/lib/denpie --create-home --shell /usr/sbin/nologin denpie

WORKDIR /app
COPY --from=builder /app/target/release/denpie /usr/local/bin/denpie
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
