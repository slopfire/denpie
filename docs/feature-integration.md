# Feature Integration Guide

Add new behavior here:

1. `src/domain/` — pure rules (scheduling, review, tipcard/topic logic). No SQL or YAML.
2. `src/services/` — orchestration. Services may call repositories, domain, LLM, settings.
3. `src/db/repositories/` — bound-parameter SQL. Row structs live next to their queries.
4. `src/api/`, `src/dashboard/`, `src/auth.rs` — thin HTTP handlers. Translate transport into service calls.
5. `src/types.rs` — shared request/response types used by handlers and services.

## New Protobuf Operation

1. Add request/response messages to `proto/denpie.proto`.
2. Build so `build.rs` regenerates Rust code.
3. Add dispatch in `src/api/transport.rs`.
4. Put shared logic in `src/services/` when the dashboard or other callers need it.
5. Add a `POST /api` integration test covering auth, success, and the main failure path.

## New Dashboard Endpoint

1. Add the route in `src/app.rs`.
2. Keep parsing and response shaping in `src/dashboard/handlers.rs`.
3. Reuse the same service as the protobuf operation when behavior overlaps.
4. Add a session-backed HTTP test.

## New Frontend Strings

1. Put dashboard copy in `frontend/src/i18n/en.json` and read it via `use_i18n().t("group.key")`.
2. Use `use_i18n().tf("group.key", &[("name", value)])` for placeholders.
3. Group keys by surface: `nav.*`, `auth.*`, `toast.*`, `confirm.*`, `api_keys.*`.
4. Use translated strings for frontend-authored toasts/confirms. Backend error bodies may still be shown as-is.
5. Do not translate protocol/storage identifiers (`tipcard_type`, review actions, roles, route paths, localStorage keys, MIME types, API enum values). Map them to display labels at the UI boundary.

## New Database Field

1. Update `schema.sql` for fresh installs.
2. Add compatibility migration code in `src/db/migrations.rs`.
3. Update repository row structs and bound SQL.
4. Add migration coverage for both fresh and old database shapes.

## New Scheduled Job

1. Put the scheduler loop in its own module (e.g., `src/daily_refresh.rs`).
2. Put the work in a function that can be called once in tests.
3. Use `tracing` for start, skip, success, and failure events.
4. Avoid long transactions while calling external services.
