---
name: add-feature
description: >
  Place new Denpie work in the correct layer and follow integration checklists for
  protobuf, dashboard, frontend, DB, and jobs. Use when adding a feature, endpoint,
  domain rule, repository, migration, Astro island, shadcn, or scheduled job.
  Triggers: new feature, add endpoint, new proto, migration, Astro island,
  shadcn, where does this go.
---

# Add feature (Denpie)

Full detail: `docs/feature-integration.md`. Browser UI: `docs/frontend-astro.md`.
Use CodeGraph first if the area is unfamiliar.

## Pick a layer

| Need | Path | Rule |
|---|---|---|
| Pure rule | `src/domain/` | No SQL, no YAML |
| Orchestration | `src/services/` | Repos, domain, LLM, settings |
| SQL | `src/db/repositories/` | Bound params only; row structs next to queries |
| Protobuf API | `src/api/` | Thin: transport → service |
| Browser/dashboard | `src/dashboard/` | Thin handlers; call services |
| Auth transport | `src/auth.rs` | Keep thin |
| Shared shapes | `src/types.rs` | Request/response types |
| New DB shape | `schema.sql` + `migrations/` | Test fresh DB and the PostgreSQL upgrade path |

### Also know these homes

| What | Path |
|---|---|
| LLM | `src/llm/` |
| Scheduling | `src/scheduling/` SM-2 only (`FSRS` is a legacy alias) |
| Tests | `src/tests/` |
| Router / entry | `src/app.rs` / `src/lib.rs` / `src/main.rs` |
| Settings | `src/config/` |
| Daily worker | `src/daily_refresh.rs` |
| Images | `src/image_compress.rs`, `src/image_store.rs` |
| Lab | `src/lab/`, `lab/cases/`, `just lab`. Opt-in research, not CI |
| Proto | `proto/denpie.proto` |
| Browser UI | `frontend-astro/`. Static Astro + React islands. |

## Checklists

### New protobuf operation

1. Messages in `proto/denpie.proto`
2. Register request/result/auth/scope/mutation policy in `api/operations-v1.json`
3. Build so `build.rs` regenerates Rust and its runtime result assertion
4. Dispatch in `src/api/transport.rs`
5. Shared logic → `src/services/` when dashboard or others need it
6. Integration test on `POST /api/v1`: auth, idempotency when mutating, success,
   structured main failure path
7. Update docs/examples/changelog, then run `just api-contract-update` and
   `just api-check`. Breaking v1 changes require a new major API; see
   `docs/api-development-rules.md`.

### Additive field on an existing protobuf result

1. Trace the exact producer and consumer boundary and prove which existing
   result drops the needed state. Do not add a parallel endpoint when an
   additive field expresses the same contract.
2. Use the next never-used field number in `proto/denpie.proto`; do not recycle
   removed numbers.
3. Populate it in the backend mapper and decode it in every current frontend
   mapper that consumes the result.
4. Add a backend transport/integration assertion and a focused frontend mapping
   test. Keep test-only exports `pub(crate)`.
5. Update API semantics, protobuf docs, and changelog.
6. Run `just api-contract-update`, inspect the ledger diff for only the intended
   additive field, then run `just api-check`.

### New dashboard endpoint

1. Route in `src/app.rs`
2. Parse + shape in `src/dashboard/handlers.rs` (or handlers submodules)
3. Same service as the protobuf op when behavior overlaps
4. Session-backed HTTP test

### New frontend strings

Shared catalog: `frontend-astro/src/i18n/en.json`.

1. Add the key under the surface group (`nav.*`, `auth.*`, `toast.*`, `confirm.*`, …)
2. Astro: `t("group.key")` or `tf("group.key", { name: value })` from `@/lib/i18n`
3. Map protocol/storage IDs at the UI edge. Unknown values stay visible as the raw ID.
4. `just frontend-astro-i18n-check` must stay clean

### New Astro UI

1. Route in `frontend-astro/src/pages/`. Shell mapping in `src/islands/AppShell.tsx`. Screen in `src/components/pages/` or `src/components/flow/`.
2. Transitions in `src/lib/` as pure functions. Requests start from effects or event handlers.
3. Registry primitives: from `frontend-astro/`, `bunx --bun shadcn@latest add <name>`. Keep `shadcn diff` clean.
4. Semantic Tailwind only. New tokens: every `[data-theme]` block in `src/styles/global.css`.
5. Generated wire types in `src/generated/denpie_pb.ts`. Flow helpers in `src/lib/api-v1/ops.ts`; other routes in `route-ops.ts`. Card IDs stay `bigint`.
6. `just frontend-astro-test`. Visible UI: `just ui-check` then `just playwright` on `:3027`.

Detail: `docs/frontend-astro.md`.

### New database field

1. `schema.sql` (fresh installs)
2. Forward-only PostgreSQL migration in `migrations/`
3. Repository row structs + bound SQL
4. Tests for the fresh schema and upgrade path

### New scheduled job

1. Own module (pattern: `src/daily_refresh.rs`)
2. One-shot function callable from tests
3. `tracing` for start / skip / success / failure
4. No long transactions while calling external services

## Docs rule

Change code → update docs and examples in the same change (`docs/`, agent-server guide, protobuf API as needed).
