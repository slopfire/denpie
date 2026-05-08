# AI Agent Instructions (Denpie)

**Denpie** codebase. Primary context guide.

## Project Overview
Backend service for daily tip cards. Scheduling truth now SM-2. No claim real FSRS until code has real FSRS. Admin dashboard, API key auth, Gemini/OpenAI-compatible tips via `async-openai`.

## Technology Stack & Best Practices
- **Language**: Rust (edition 2021)
- **Web Framework**: Axum (`tower-http`, `tower-sessions`)
- **Database**: SQLite via SQLx
  - **CRITICAL**: Use safe query binding in SQLx. No SQL injection.
  - Review `schema.sql` for table structure (`api_keys`, `topics`, `tipcards`, `review_states`).
- **Configuration**: YAML (`settings.yaml` for LLM parameters)
- **Schema**: `migrations/` snapshots plus compatibility `schema.sql`; startup migration helpers live in `src/db/migrations.rs`.
- **Async Runtime**: Tokio
- **LLM Integration**: `async-openai` (Gemini endpoint)
- **Frontend**: Tailwind CSS (Admin UI)

## Architecture & File Mapping
- **Design Paradigm**: Single-user, multi-client. Global scheduling state; multiple clients (desktop widget, Telegram bot) via API keys.
- `src/main.rs`: Axum router, DI (State), DB pool, app init.
- `src/config/`: typed settings + YAML store. Raw YAML `Value` stay here, ugh.
- `src/db/repositories/`: SQL lives here as refactor grows. Bind params, no injection nonsense.
- `src/domain/`: scheduling/review/tipcard rules. No SQL/YAML.
- `src/services/`: orchestration for settings and API keys, more services later.
- `src/api.rs`: API module exports. Small.
- `src/api/`: protobuf transport, request types, tip generation, admin/topic/tipcard helpers.
- `src/auth.rs`: session middleware/login transport; API key verify through service.
- `src/dashboard.rs`: browser handlers; settings/key calls through services.
- `src/srs.rs`: SM-2 scheduling implementation.
- `src/llm.rs`: LLM wrappers for Gemini API.

## Persona & Behavioral Rules (CRITICAL)
1. **Communication Mode**: Normal chat → **sassy caveman full mode** (e.g. "Me do thing. You want? Ugh.").
2. **Documentation**: `README.md` → full English, no caveman. Agent `.md` files → caveman mode.
3. **Tool Usage**: Prefer MCP tools. No `bash` for file viewing/editing if dedicated tools exist.
4. **Update docs**: Modify code → update docs and examples.

## Development Workflow
```bash
cargo check  # verify compilation without running
cargo run    # start server on 127.0.0.1:3017
```
Use chrome dev tools if you want to check something on website
Remember to close cargo run to allow me to test everything by myself
`schema.sql` auto-runs on startup to ensure tables exist.
