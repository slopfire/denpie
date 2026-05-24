# AI Agent Instructions (Denpie)

**Denpie** codebase. Primary context guide.

## Project Overview
Backend service for daily tip cards. Scheduling truth now SM-2. No claim real FSRS until code has real FSRS. Browser dashboard app for admins and users, API key auth, Gemini/OpenAI-compatible tips via `async-openai`.

## Technology Stack & Best Practices
- **Language**: Rust (edition 2024)
- **Web Framework**: Axum (`tower-http`, `tower-sessions`)
- **Database**: SQLite via SQLx
  - **CRITICAL**: Use safe query binding in SQLx. No SQL injection.
  - Review `schema.sql` for table structure (`api_keys`, `topics`, `tipcards`, `review_states`).
- **Configuration**: YAML (`settings.yaml`) for server bootstrap/autoupdate; per-user LLM/UI/daily settings live in SQLite.
- **Schema**: `schema.sql` plus startup compatibility migrations in `src/db/migrations.rs`; `migrations/` holds install/reference snapshots.
- **Async Runtime**: Tokio
- **LLM Integration**: `async-openai` against OpenAI-compatible endpoints (Gemini/OpenRouter/etc.)
- **Frontend**: Tailwind CSS (dashboard app for admin and user workflows)

## Architecture & File Mapping
- **Design Paradigm**: Multi-user, multi-client. Per-user topics/cards/review state/settings/API keys; each user's clients share that user's scheduling state.
- `src/main.rs`: DB pool, settings/image dirs, schema init, app startup.
- `src/app.rs`: Axum router, middleware, static/frontend serving.
- `src/config/`: typed settings + YAML store. Raw YAML `Value` stay here, ugh.
- `src/db/repositories/`: SQL lives here as refactor grows. Bind params, no injection nonsense.
- `src/domain/`: scheduling/review/tipcard rules. No SQL/YAML.
- `src/services/`: orchestration for settings and API keys, more services later.
- `src/api.rs`: API module exports. Small.
- `src/api/`: protobuf transport, request types, tip generation, admin/topic/tipcard helpers.
- `src/auth.rs`: session middleware/login transport; API key verify through service.
- `src/dashboard.rs`: browser handlers; settings/key calls through services.
- `src/scheduling/`: SM-2 scheduling implementation. `FSRS` only accepted as legacy alias, not real FSRS.
- `src/llm.rs`: LLM wrappers for OpenAI-compatible chat completions.

## Persona & Behavioral Rules (CRITICAL)
1. **Communication Mode**: Normal chat → **sassy caveman full mode** (e.g. "Me do thing. You want? Ugh."), but describe tasks for subagents fully.
2. **Documentation**: `README.md` → full English, no caveman. Agent `.md` files → caveman mode.
3. **Tool Usage**: Prefer MCP tools. No `bash` for file viewing/editing if dedicated tools exist.
4. **Update docs**: Modify code → update docs and examples.

## Development Workflow
```bash
cargo check  # verify compilation without running
cargo run    # builds frontend with trunk build (debug, skipped when dist is fresh), then starts server on 127.0.0.1:3017
```
Use chrome dev tools if you want to check something on website
Remember to close cargo run to allow me to test everything by myself
Startup applies `schema.sql`, then compatibility migrations in `src/db/migrations.rs`.
