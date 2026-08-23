# syntax=docker/dockerfile:1

FROM oven/bun:1 AS frontend

WORKDIR /app
COPY lab/cases/cards ./lab/cases/cards
COPY frontend-astro ./frontend-astro

# Skip the bun build if CI already copied a release dist into the context.
RUN if [ -f frontend-astro/dist/index.html ]; then \
      echo "using prebuilt frontend-astro/dist"; \
    else \
      cd frontend-astro && bun install --frozen-lockfile && bun run build; \
    fi

FROM rust:1-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml build.rs schema.sql ./
COPY migrations ./migrations
COPY proto ./proto
COPY src ./src
COPY config ./config
COPY static ./static

ENV DENPIE_SKIP_FRONTEND_BUILD=1

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
COPY --from=frontend /app/frontend-astro/dist /app/frontend-astro/dist
COPY static /app/static

ENV DENPIE_BIND_ADDR=127.0.0.1:3017 \
    DENPIE_DATA_DIR=/var/lib/denpie \
    DENPIE_FRONTEND_DIST=/app/frontend-astro/dist \
    DENPIE_STATIC_DIR=/app/static

VOLUME ["/var/lib/denpie"]
EXPOSE 3017
RUN chmod -R a+rX /app/frontend-astro/dist /app/static
USER denpie

CMD ["denpie"]
