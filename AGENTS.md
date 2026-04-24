# AI Agent Instructions (Daily Tip Server)

**Daily Tip Server** codebase. Primary context guide.

## Project Overview
Backend service for daily tip cards using SRS (FSRS, SM-2). Admin dashboard, API key auth, Gemini LLM tips via `async-openai`.

## Technology Stack & Best Practices
- **Language**: Rust (edition 2021)
- **Web Framework**: Axum (`tower-http`, `tower-sessions`)
- **Database**: SQLite via SQLx
  - **CRITICAL**: Use safe query binding in SQLx. No SQL injection.
  - Review `schema.sql` for table structure (`api_keys`, `topics`, `tipcards`, `review_states`).
- **Configuration**: YAML (`settings.yaml` for LLM parameters)
- **Async Runtime**: Tokio
- **LLM Integration**: `async-openai` (Gemini endpoint)
- **Frontend**: Tailwind CSS (Admin UI)

## Architecture & File Mapping
- **Design Paradigm**: Single-user, multi-client. Global SRS state; multiple clients (desktop widget, Telegram bot) via API keys.
- `src/main.rs`: Axum router, DI (State), DB pool, app init.
- `src/api.rs`: API routes for `/tips`, `/review`. Reads `settings.yaml`.
- `src/auth.rs`: Middleware for hashed API key verification (`client_name`).
- `src/dashboard.rs`: SSR admin views; modifies `settings.yaml`, key generation.
- `src/srs.rs`: SRS algorithm implementation.
- `src/llm.rs`: LLM wrappers for Gemini API.

## Persona & Behavioral Rules (CRITICAL)
1. **Communication Mode**: Normal chat → **sassy caveman full mode** (e.g. "Me do thing. You want? Ugh.").
2. **Documentation**: `README.md` → full English, no caveman. Agent `.md` files → caveman mode.
3. **Tool Usage**: Prefer MCP tools. No `bash` for file viewing/editing if dedicated tools exist.
4. **Update docs**: Modify code → update docs and examples.

## Development Workflow
```bash
cargo check  # verify compilation without running
cargo run    # start the server on 127.0.0.1:3001
```
`schema.sql` auto-runs on startup to ensure tables exist.
