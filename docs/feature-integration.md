# Feature Integration Guide

Use this path for new behavior:

1. Put pure rules in `src/domain/`.
2. Put orchestration in `src/services/`.
3. Put SQL in `src/db/repositories/`.
4. Keep HTTP handlers in `src/api/`, `src/dashboard.rs`, and `src/auth.rs` thin.

Handlers should translate transport details into service calls. Services may combine repositories, settings, LLM calls, scheduling, and domain rules. Repositories should stay boring: bound SQL parameters in, typed rows out.

## New Protobuf Operation

1. Add the request and response messages to `proto/denpie.proto`.
2. Run the normal Cargo build so `build.rs` regenerates protobuf Rust code.
3. Add dispatch in `src/api/transport.rs`.
4. Put shared behavior in a service when the dashboard or another caller needs the same logic.
5. Add a `POST /api` integration test that covers auth, success, and the most important failure.

## New Dashboard Endpoint

1. Add the route in `src/app.rs`.
2. Keep request parsing and response shaping in `src/dashboard.rs`.
3. Reuse the same service path as protobuf operations when behavior overlaps.
4. Add a session-backed HTTP test for the endpoint.

## New Frontend Strings

1. Put user-facing dashboard copy in `frontend/src/i18n/en.json` and read it through `use_i18n().t("group.key")`.
2. Use `use_i18n().tf("group.key", &[("name", value)])` for strings with placeholders such as counts or HTTP status text.
3. Keep keys grouped by surface (`nav.*`, `auth.*`, `toast.*`, `confirm.*`, `api_keys.*`) so translators can work one page at a time.
4. Use translated toast and confirm strings for frontend-authored messages. Backend error bodies may still be shown as-is until backend message codes are added.
5. Do not translate protocol or storage identifiers such as `tipcard_type`, review actions, roles, route paths, localStorage keys, MIME types, or API enum values. Map those values to translated display labels at the UI boundary instead.

## New Database Field

1. Update `schema.sql` for fresh installs.
2. Add compatibility migration code in `src/db/migrations.rs` for existing databases.
3. Update repository row structs and bound SQL.
4. Add migration coverage for both fresh and old database shapes.

## New Scheduled Job

1. Put the scheduler loop in its own module.
2. Put the actual work in a function that can run once in tests.
3. Use `tracing` for start, skip, success, and failure events.
4. Avoid holding long transactions while calling external services.
