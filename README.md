# Daily Tip Server

A Rust-based backend service that generates, serves, and schedules daily tip cards using a spaced repetition system (SM-2 / FSRS). LLM content generation is powered by any OpenAI-compatible API endpoint (Gemini, OpenRouter, etc.) via `async-openai`.

## Features

- **Spaced Repetition System (SRS)**: SM-2 and FSRS algorithms optimize tip delivery based on user review grades.
- **Any OpenAI-Compatible LLM**: Configure the API key, base URL, and model through the admin dashboard — no hardcoded vendor lock-in.
- **Admin Dashboard**: Web UI for managing LLM settings, prompt templates, and client API keys. All settings persist to `settings.yaml`.
- **Token-Based Admin Auth**: On first startup the server generates and prints a one-time admin token. Use it to log in at `/admin`.
- **Protobuf API**: Tips and reviews are exchanged as Protocol Buffers for compact, typed serialization.
- **Single-User, Multi-Client**: One user's SRS state is shared across all clients (desktop widget, Telegram bot, etc.) via per-client API keys.
- **SQLite Database**: Lightweight persistence via `sqlx` with compile-time query validation.

## Tech Stack

| Layer | Technology |
|---|---|
| Language | Rust (edition 2021) |
| Web Framework | Axum |
| Database | SQLite (via SQLx) |
| Async Runtime | Tokio |
| LLM Client | `async-openai` |
| Serialization | Protocol Buffers (`prost`) |
| Frontend | Tailwind CSS (CDN, admin UI only) |

## Project Structure

```
.
├── src/
│   ├── main.rs        # Router setup, state, app initialization, integration tests
│   ├── api.rs         # /tips and /review endpoints (protobuf)
│   ├── auth.rs        # Admin session middleware + client API key validation
│   ├── dashboard.rs   # Admin HTML page + settings/key management REST endpoints
│   ├── llm.rs         # LLM wrappers (generate_new_card, compress_card)
│   └── srs.rs         # SM-2 and FSRS algorithm implementations
├── schema.sql         # SQLite table definitions (applied automatically on startup)
├── settings.yaml      # Runtime config — excluded from version control
├── .env.example       # Environment variable template
└── dailytip.proto     # Protobuf schema for TipsQuery, TipsResponse, ReviewPayload
```

## Getting Started

### Prerequisites

- Rust (latest stable)
- SQLite

### Setup

1. **Clone the repository.**

2. **Configure environment** (only needed for SQLx compile-time checks):
   ```bash
   cp .env.example .env
   ```
   The only required variable is `DATABASE_URL`. LLM credentials are **not** set via environment variables.

3. **Run the server:**
   ```bash
   cargo run
   ```
   The server starts on `http://127.0.0.1:3000`. On the first run it will:
   - Create `dailytip.db` and apply `schema.sql` automatically.
   - Generate and print a one-time admin token to the console.

4. **Access the admin dashboard** at `http://localhost:3000/admin` and log in with the printed token.

5. **Configure your LLM** in the Configuration panel:
   - **LLM Model** — e.g. `google/gemini-2.5-pro` or `openai/gpt-4o`
   - **LLM API Key** — your provider API key
   - **LLM Base URL** — e.g. `https://openrouter.ai/api/v1` or `https://generativelanguage.googleapis.com/v1beta/openai`
   - **Prompt Template** — use `{topic}` as the placeholder

6. **Generate a client API key** in the Access Keys panel. Pass it as the `Authorization` header on every `/tips` and `/review` request.

## Configuration (`settings.yaml`)

All runtime configuration lives in `settings.yaml` and is managed exclusively through the dashboard. **Do not commit this file** — it contains your API key and admin token.

| Key | Description | Default |
|---|---|---|
| `admin_token` | Hashed token for dashboard login | Auto-generated on first run |
| `llm_model` | Model identifier string | `google/gemini-3.1-flash` |
| `prompt_template` | Tip generation prompt (`{topic}` placeholder) | `Give a smart tip about {topic}.` |
| `llm_api_key` | API key for the LLM provider | *(empty — set via dashboard)* |
| `llm_base_url` | Base URL for the OpenAI-compatible API | `https://openrouter.ai/api/v1` |

## API Reference

All client endpoints require an `Authorization: <api_key>` header.

### `POST /tips`

Body: `TipsQuery` (protobuf)

```proto
message TipsQuery {
  uint32 count  = 1;  // number of tips to return
  string topics = 2;  // comma-separated topic names
}
```

Returns: `TipsResponse` (protobuf) containing a list of `TipCardResponse` messages.

### `POST /review`

Body: `ReviewPayload` (protobuf)

```proto
message ReviewPayload {
  int64  card_id = 1;
  uint32 grade   = 2;  // 0–5 (Anki-style)
}
```

Returns: `200 OK` on success, `404` if the card has no review state.

## Database Schema

| Table | Purpose |
|---|---|
| `api_keys` | Hashed client keys with display names |
| `topics` | Topic categories |
| `tipcards` | Generated tips (full + compressed content) |
| `review_states` | Per-card SRS state and next review timestamp |

## Running Tests

```bash
cargo test
```

The test suite spawns a real server on an ephemeral port for each test group and exercises auth, settings CRUD, key management, the full tips→review flow, and error handling.

> **Note:** The `test_full_api_flow` test calls the real LLM endpoint. Set `llm_api_key` in `settings.yaml` before running it, or expect the tip content to contain `API KEY MISSING`.
