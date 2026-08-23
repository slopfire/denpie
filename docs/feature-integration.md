# Feature integration

Match the work to a layer. Put the code there.

Agent procedure: [`.agents/skills/add-feature`](../.agents/skills/add-feature/SKILL.md). Browser UI: [`frontend-astro.md`](frontend-astro.md).

| Step | Path | Rule |
|---|---|---|
| 1 | `src/domain/` | Pure rules only. No SQL. No YAML. |
| 2 | `src/services/` | Orchestration: repos, domain, LLM, settings. |
| 3 | `src/db/repositories/` | Bound-parameter SQL. Row structs next to queries. |
| 4 | `src/api/`, `src/dashboard/`, `src/auth.rs` | Thin handlers. Transport → service call. |
| 5 | `src/types.rs` | Shared request/response shapes. |

## Other homes

| What | Path | Rule |
|---|---|---|
| Lab code | `src/lab/` | Opt-in research benches only; no server routes, no DB, no CI wiring |
| Lab data | `lab/cases/` | Checked-in case packs such as the image gold set |
| Lab entry | `just lab` | Opt-in runner; never called from `just test` / `just verify` / `just ci` |
| Lab contract | `just lab-check` | Deterministic offline CLI, fixture, comparison, and lab-UI checks |
| Card UI lab | `frontend-astro/src/pages/lab-cards.astro` | `/lab-cards` mounts production cards on fixtures; no server writes. |
| Browser UI | `frontend-astro/` | Static Astro + React islands. Detail: [`frontend-astro.md`](frontend-astro.md). |

## New protobuf operation

1. Add request/response messages in `proto/denpie.proto`
2. Build so `build.rs` regenerates Rust
3. Dispatch in `src/api/transport.rs`
4. Shared logic → `src/services/` when dashboard or others need it
5. Integration test on `POST /api`: auth, success, main failure path

## New dashboard endpoint

1. Route in `src/app.rs`
2. Parse + shape response in `src/dashboard/handlers.rs`
3. Reuse the same service as the protobuf op when behavior overlaps
4. Session-backed HTTP test

## New frontend strings

Shared catalog: `frontend-astro/src/i18n/en.json`. Steps: [`frontend-astro.md`](frontend-astro.md#add-a-string).

## New Astro UI

Put browser UI in `frontend-astro/`. Steps and completion criteria: [`frontend-astro.md`](frontend-astro.md).

## New database field

1. Update `schema.sql` (canonical fresh schema)
2. Add a forward-only PostgreSQL migration in `migrations/`
3. Update repository row structs + bound SQL
4. Cover the fresh schema and the upgrade path in PostgreSQL tests

## New scheduled job

1. Scheduler loop in its own module (pattern: `src/daily_refresh.rs`)
2. Work in a function you can call once from tests
3. `tracing` for start, skip, success, failure
4. No long transactions while calling external services
