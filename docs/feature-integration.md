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

## New Frontend UI Component

The frontend is **Yew/WASM**, not React, so the `shadcn` CLI (`init`/`add`) does not apply — it ships React/TSX source. Instead, this project is a **shadcn token-port**: shadcn's CSS variable system and component conventions, implemented in Yew + Tailwind v4.

### How the token system works

1. `frontend/index.html` defines shadcn CSS variables per theme (`--background`, `--foreground`, `--primary-hsl`, `--destructive-hsl`, `--border-hsl`, `--radius`, …) inside `[data-theme="..."]` blocks.
2. A separate `<style type="text/tailwindcss">` block contains an `@theme inline` mapping that turns those variables into Tailwind v4 utilities: `bg-primary`, `text-foreground`, `border-border`, `bg-destructive`, `text-muted-foreground`, `rounded-md`, etc.
3. Components use **semantic utilities** (`bg-primary`, `text-destructive`) — never raw colors (`bg-red-600`, `dark:bg-red-900/30`) and never manual `dark:` overrides.

### Adding a Yew UI primitive

1. Create `frontend/src/components/<name>.rs` with a `Shadcn<Name>` component (see `button.rs`, `select.rs`, `tooltip.rs`).
2. Expose variants/sizes as enums mirroring shadcn's variant system; map each to semantic utility classes.
3. Accept a `class: Classes` prop for layout overrides and a `children: Children` prop for content.
4. Register the module in `frontend/src/components/mod.rs`.
5. Use semantic tokens only. If a token is missing, add the CSS variable to every `[data-theme]` block and the `@theme inline` mapping — not a one-off color.

### When to add a new token

If a component needs a color not covered by `primary`/`secondary`/`muted`/`accent`/`destructive`/`border`/`input`/`ring`/`card`/`popover`/`foreground`/`background`, add it to **every** `[data-theme="..."]` block in `frontend/index.html` (including `--<name>-foreground` where applicable) and to the `@theme inline` block so Tailwind generates the `bg-<name>` / `text-<name>` utilities.

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
