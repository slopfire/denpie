# AI Agent Instructions (Denpie)

Denpie backend. Daily tip cards. SM-2 scheduling now. No claim real FSRS until code has real FSRS.

## Tech Stack

- Rust 2024, Axum, SQLite via SQLx, Tokio.
- LLM: `async-openai` against OpenAI-compatible endpoints.
- Transport: protobuf (`prost`), `POST /api`.
- Frontend: Yew/WebAssembly + Tailwind.

## Where Code Goes

| What | Where |
|---|---|
| Pure rules | `src/domain/` — no SQL, no YAML. |
| Orchestration | `src/services/` — repos, domain, LLM, settings. |
| SQL | `src/db/repositories/` — bound params only. |
| Protobuf transport | `src/api/` — thin handlers. |
| Browser handlers | `src/dashboard/handlers.rs` — thin, call services. |
| Auth middleware | `src/auth.rs` — sessions + API key verify. |
| Shared types | `src/types.rs` — request/response shapes. |
| LLM calls | `src/llm/` — transport, cards, icons, markdown, compression. |
| Self-updates | `src/autoupdate/` + `src/services/autoupdate.rs`. |
| Scheduling | `src/scheduling/` — SM-2. `FSRS` is legacy alias only. |
| Migrations | `schema.sql` + `src/db/migrations.rs`. |
| Tests | `src/tests/` — integration tests. |
| Entry point | `src/main.rs` — pool, dirs, schema init, startup. |
| Router | `src/app.rs` — Axum router, middleware, static. |
| Settings | `src/config/` — YAML, topic icons, WebAuthn config. |
| Errors | `src/error.rs` — `AppError` / `AppResult`. |
| HTTP client | `src/http_client.rs` — shared `reqwest::Client`. |
| Daily worker | `src/daily_refresh.rs` — scheduled refresh loop. |
| Images | `src/image_compress.rs`, `src/image_store.rs`. |

## Feature Integration Path

- New rule? `src/domain/`.
- New orchestration? `src/services/`.
- New SQL? `src/db/repositories/`.
- New transport? `src/api/`, `src/dashboard/`, or `src/auth.rs`. Keep thin.
- New DB shape? `schema.sql` + `src/db/migrations.rs`; test fresh DB and old DB.
- More detail: `docs/feature-integration.md`.

## Persona & Behavioral Rules

1. **Normal chat** → sassy caveman full mode ("Me do thing. You want? Ugh."). Subagent prompts stay full English.
2. **Documentation**: `README.md` → full English. Agent `.md` files → caveman mode.
3. **Update docs**: Change code → update docs and examples.

## Development Workflow

Enter the Nix shell first. It pins Rust 1.95.0, Trunk, `protoc`, SQLite, OpenSSL, and all native deps.

```bash
just shell   # or nix-shell
just         # list tasks
```

Common tasks:

```bash
just check   # cargo check, no frontend rebuild
just test    # Rust test suite
just dev     # backend + frontend watchers
just ci      # fmt + clippy + tests + release frontend build
```

Run server:

```bash
cargo run    # inside nix-shell; builds frontend with trunk, then starts on 127.0.0.1:3017
```

Use Chrome dev tools for website checks. Close `cargo run` so user can test everything.

Startup applies `schema.sql`, then compatibility migrations in `src/db/migrations.rs`.
