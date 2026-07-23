# AI Agent Instructions (Denpie)

Daily tip cards. SM-2 scheduling. Do not claim real FSRS until the code has real FSRS.

## Start here

```bash
just shell   # or nix-shell — pins Rust 1.95.0, Trunk, protoc, SQLite, OpenSSL
just         # list tasks
```

```bash
just check   # cargo check, no frontend rebuild
just test    # Rust tests
just dev     # backend + frontend watchers
just ci      # fmt + clippy + tests + release frontend build
```

## Agent server (port rules)

```bash
DENPIE_BIND_ADDR=127.0.0.1:3027 DENPIE_RP_ORIGIN=http://localhost:3027 cargo run
```

1. **Never touch `:3017`** — user port. Do not bind, reuse, inspect, restart, or stop it.
2. **UI checks → `http://localhost:3027` only.** Overrides any skill that says reuse `:3017`.
3. Check `:3027` first. Reuse Denpie if already running. Never kill a pre-existing process.
4. Stop only servers you started. Close your own `cargo run` when done.
5. UI checks: concrete DOM IDs and placement. Skip brittle a11y names and `waitForUrl` for SPA routes.

Simple visible UI change → `just check` + targeted DOM check. Stop if the user sees it or waives the check. No over-verification unless asked.

Startup: `schema.sql`, then compatibility migrations in `src/db/migrations.rs`.

## Test login

- Username: `test`
- Password: `23452345`

## Where code goes

**Reach for these first**

| What | Path |
|---|---|
| Pure rules | `src/domain/` — no SQL, no YAML |
| Orchestration | `src/services/` — repos, domain, LLM, settings |
| SQL | `src/db/repositories/` — bound params only |
| Protobuf handlers | `src/api/` — thin |
| Browser handlers | `src/dashboard/handlers.rs` — thin, call services |

**Also**

| What | Path |
|---|---|
| Auth | `src/auth.rs` |
| Shared types | `src/types.rs` |
| LLM | `src/llm/` |
| Self-updates | `src/autoupdate/` + `src/services/autoupdate.rs` |
| Scheduling | `src/scheduling/` — SM-2 only (`FSRS` = legacy alias) |
| Migrations | `schema.sql` + `src/db/migrations.rs` |
| Tests | `src/tests/` |
| Entry / router | `src/main.rs` / `src/app.rs` |
| Settings | `src/config/` |
| Errors | `src/error.rs` |
| HTTP client | `src/http_client.rs` |
| Daily worker | `src/daily_refresh.rs` |
| Images | `src/image_compress.rs`, `src/image_store.rs` |

## New feature — pick one path

| Need | Put it in |
|---|---|
| New rule | `src/domain/` |
| New orchestration | `src/services/` |
| New SQL | `src/db/repositories/` |
| New transport | `src/api/`, `src/dashboard/`, or `src/auth.rs` (keep thin) |
| New DB shape | `schema.sql` + `src/db/migrations.rs` — test fresh DB and old DB |

Detail: [`docs/feature-integration.md`](docs/feature-integration.md).

## Docs rule

Change code → update docs and examples in the same change.

## Stack

- Rust 2024, Axum, SQLite via SQLx, Tokio
- LLM: `async-openai` against OpenAI-compatible endpoints
- Transport: protobuf (`prost`), `POST /api`
- Frontend: Yew/WebAssembly + Tailwind
