# Feature Integration Guide

Match the work to a layer. Put the code there.

Agent procedure (same content, checklist form): [`.agents/skills/add-feature`](../.agents/skills/add-feature/SKILL.md).

| Step | Path | Rule |
|---|---|---|
| 1 | `src/domain/` | Pure rules only. No SQL. No YAML. |
| 2 | `src/services/` | Orchestration: repos, domain, LLM, settings. |
| 3 | `src/db/repositories/` | Bound-parameter SQL. Row structs next to queries. |
| 4 | `src/api/`, `src/dashboard/`, `src/auth.rs` | Thin handlers. Transport → service call. |
| 5 | `src/types.rs` | Shared request/response shapes. |

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

1. Copy in `frontend/src/i18n/en.json` → `use_i18n().t("group.key")`
2. Placeholders → `use_i18n().tf("group.key", &[("name", value)])`
3. Group by surface: `nav.*`, `auth.*`, `toast.*`, `confirm.*`, `api_keys.*`
4. Frontend-authored toasts/confirms: translated strings. Backend error bodies may show as-is.
5. Error toasts stay visible until the user dismisses them; success and info toasts dismiss automatically.
6. Do **not** translate protocol/storage IDs (`tipcard_type`, review actions, roles, routes, localStorage keys, MIME types, API enums). Map to labels at the UI edge.

## New Yew UI component

Frontend is **Yew/WASM**, not React. No `shadcn` CLI. This repo is a **shadcn token-port**: same CSS variables + conventions, Yew + Tailwind v4.

### Tokens

1. `frontend/index.html` — shadcn CSS vars per theme in `[data-theme="..."]` blocks
2. `@theme inline` maps vars → Tailwind utilities (`bg-primary`, `text-foreground`, …)
3. Components use **semantic** classes only — never raw colors or manual `dark:` overrides

### Add a primitive

1. Create `frontend/src/components/<name>.rs` as `Shadcn<Name>` (see `button.rs`, `select.rs`, `tooltip.rs`)
2. Variants/sizes as enums → semantic utility classes
3. Props: `class: Classes` for layout, `children: Children` for content
4. Register in `frontend/src/components/mod.rs`
5. Missing token? Add the CSS variable to **every** `[data-theme]` block and to `@theme inline` — not a one-off color

### New token

Color not covered by `primary` / `secondary` / `muted` / `accent` / `destructive` / `border` / `input` / `ring` / `card` / `popover` / `foreground` / `background`?

1. Add `--<name>` (and `--<name>-foreground` if needed) to every `[data-theme="..."]` block in `frontend/index.html`
2. Add it to `@theme inline` so Tailwind emits `bg-<name>` / `text-<name>`

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
