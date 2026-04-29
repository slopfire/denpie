FROM rust:1-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml build.rs schema.sql ./
COPY proto ./proto
COPY src ./src
COPY templates ./templates
RUN cargo build --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home /var/lib/dailytipdraft --create-home --shell /usr/sbin/nologin dailytipdraft

WORKDIR /app
COPY --from=builder /app/target/release/dailytipdraft /usr/local/bin/dailytipdraft
COPY schema.sql /app/schema.sql
COPY templates /app/templates

ENV DAILYTIP_BIND_ADDR=127.0.0.1:3001 \
    DAILYTIP_DATA_DIR=/var/lib/dailytipdraft \
    DAILYTIP_SCHEMA_PATH=/app/schema.sql

VOLUME ["/var/lib/dailytipdraft"]
EXPOSE 3001
USER dailytipdraft

CMD ["dailytipdraft"]
