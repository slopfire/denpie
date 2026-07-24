---
name: add-feature
description: >
  Place new Denpie work in the correct layer and follow integration checklists for
  protobuf, dashboard, frontend, DB, and jobs. Use when adding a feature, endpoint,
  domain rule, repository, migration, UI component, or scheduled job. Triggers:
  new feature, add endpoint, new proto, migration, Yew component, where does this go.
---

# Add feature (Denpie)

Full detail: `docs/feature-integration.md`. Use CodeGraph first if the area is unfamiliar.

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
| New DB shape | `schema.sql` + `src/db/migrations.rs` | Test fresh DB and old DB |

### Also know these homes

| What | Path |
|---|---|
| LLM | `src/llm/` |
| Scheduling | `src/scheduling/` — SM-2 only (`FSRS` = legacy alias; do not claim real FSRS) |
| Tests | `src/tests/` |
| Router / entry | `src/app.rs` / `src/main.rs` |
| Settings | `src/config/` |
| Daily worker | `src/daily_refresh.rs` |
| Images | `src/image_compress.rs`, `src/image_store.rs` |
| Proto | `proto/denpie.proto` |

## Checklists

### New protobuf operation

1. Messages in `proto/denpie.proto`
2. Build so `build.rs` regenerates Rust
3. Dispatch in `src/api/transport.rs`
4. Shared logic → `src/services/` when dashboard or others need it
5. Integration test on `POST /api`: auth, success, main failure path

### New dashboard endpoint

1. Route in `src/app.rs`
2. Parse + shape in `src/dashboard/handlers.rs` (or handlers submodules)
3. Same service as the protobuf op when behavior overlaps
4. Session-backed HTTP test

### New frontend strings

1. Copy in `frontend/src/i18n/en.json` → `use_i18n().t("group.key")`
2. Placeholders → `use_i18n().tf("group.key", &[("name", value)])`
3. Group by surface: `nav.*`, `auth.*`, `toast.*`, `confirm.*`, …
4. Do **not** translate protocol/storage IDs; map to labels at the UI edge

### New Yew UI component

Frontend is **Yew/WASM** + Tailwind v4, **shadcn token-port** (not React, no shadcn CLI).

1. `frontend/src/components/<name>.rs` as `Shadcn<Name>` (see `button.rs`, `select.rs`)
2. Semantic utility classes only — no raw colors / one-off `dark:`
3. Register in `frontend/src/components/mod.rs`
4. New tokens: every `[data-theme]` block in `frontend/index.html` + `@theme inline`

### New database field

1. `schema.sql` (fresh installs)
2. Compatibility migration in `src/db/migrations.rs`
3. Repository row structs + bound SQL
4. Tests for fresh and old DB shapes

### New scheduled job

1. Own module (pattern: `src/daily_refresh.rs`)
2. One-shot function callable from tests
3. `tracing` for start / skip / success / failure
4. No long transactions while calling external services

## Docs rule

Change code → update docs and examples in the same change (`docs/`, agent-server guide, protobuf API as needed).
