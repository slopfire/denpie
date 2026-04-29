# Daily Tip Server

A Rust-based backend service that generates, serves, and schedules daily tip cards using a spaced repetition system (SM-2 / FSRS). LLM content generation is powered by any OpenAI-compatible API endpoint (Gemini, OpenRouter, etc.) via `async-openai`.

## Features

- **Spaced Repetition System (SRS)**: SM-2 and FSRS algorithms optimize tip delivery based on user review grades.
- **Casual Cards**: Queue-style tips can be dismissed or acknowledged so clients can pull the next card immediately.
- **Repeatable Cards**: Re:word-style cards can be dismissed, repeated, memorized, or acknowledged; the browser app marks repeatable cards as New or Known, advances to the next card after repeatable review actions, and brings repeated cards back when due.
- **Topic Classes**: Topics belong to a card behavior class: casual or repeatable. SRS remains the scheduling algorithm, not a card class.
- **Any OpenAI-Compatible LLM**: Configure the API key, base URL, and model through the admin dashboard — no hardcoded vendor lock-in.
- **Admin Dashboard**: Web UI for managing LLM settings, prompt templates, and client API keys. All settings persist to `settings.yaml`.
- **Markdown Tipcards**: Browser UIs render generated tip content as sanitized markdown while API responses keep the original raw text.
- **Browser Client App**: The server root (`/`) opens a session-backed MindLift SRS application for dashboard stats, unified flow over active cards, compact/expandable and copyable card text, drag-and-drop card ordering, grid/column layout switching, card deletion, API key management, settings, archive browsing, and configurable color schemes.
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
│   ├── main.rs        # Router setup, state, app initialization
│   ├── api.rs         # /tips and /review endpoints (protobuf)
│   ├── auth.rs        # Admin session middleware + client API key validation
│   ├── dashboard.rs   # Admin HTML page + settings/key management REST endpoints
│   ├── llm.rs         # LLM wrappers (generate_new_card, compress_card)
│   └── srs.rs         # SM-2 and FSRS algorithm implementations
├── templates/
│   ├── app.html       # Root browser client application
│   └── admin.html     # Legacy admin dashboard
├── schema.sql         # SQLite table definitions (applied automatically on startup)
├── proto/
│   └── dailytip.proto # Protobuf schema for TipsQuery, TipsResponse, ReviewPayload
├── docs/              # Split API documentation
└── settings.yaml      # Runtime config, generated locally and ignored
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
   The server starts on `http://127.0.0.1:3001` by default. On the first run it will:
   - Create `dailytip.db` and apply `schema.sql` automatically.
   - Generate and print a one-time admin token to the console.

4. **Open the browser client app** at `http://127.0.0.1:3001/` and log in with the printed token. The app includes the dashboard, unified flow over due active cards, compact/expandable and copyable card text, drag-and-drop card ordering, card deletion, grid/column layout switching, settings, API keys, archive, and a color scheme selector with Default, Ayu, Solarized, Dracula, and Slate themes. Adding, dismissing, repeatable memorizing/repeating, and daily casual-card refreshes show skeleton loading cards while the server generates replacements.

   The legacy admin dashboard remains available at `http://127.0.0.1:3001/admin`.

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
| `llm_compress_model` | Model identifier string for compressed summaries | `google/gemini-3.1-flash-lite-preview` |
| `llm_reasoning_effort` | Reasoning effort for generated tips: `none`, `minimal`, `low`, `medium`, `high`, or `xhigh` | `none` |
| `llm_compress_reasoning_effort` | Reasoning effort for compressed summaries; `none` disables compression thinking tokens | `none` |
| `prompt_template` | Tip generation prompt (`{topic}` placeholder) | `Give a smart tip about {topic}.` |
| `llm_api_key` | API key for the LLM provider | *(empty — set via dashboard)* |
| `llm_base_url` | Base URL for the OpenAI-compatible API | `https://openrouter.ai/api/v1` |
| `llm_compress_base_url` | Base URL for compression requests; defaults to `llm_base_url` when missing | `https://openrouter.ai/api/v1` |
| `color_scheme` | Browser client color scheme | `default` |

## Runtime Environment

The server can run from the project directory with defaults, or from an installed location with explicit paths.

| Variable | Description | Default |
|---|---|---|
| `DAILYTIP_BIND_ADDR` | Listen address and port | `127.0.0.1:3001` |
| `DAILYTIP_DATA_DIR` | Directory for `settings.yaml` and `dailytip.db` | current directory |
| `DAILYTIP_SCHEMA_PATH` | Path to `schema.sql` | `schema.sql` in the current directory |
| `DAILYTIP_TEMPLATE_DIR` | Directory containing `admin.html` and `app.html` | `templates` in the current directory |

Example:

```bash
DAILYTIP_BIND_ADDR=127.0.0.1:3001 \
DAILYTIP_DATA_DIR=/var/lib/dailytipdraft \
DAILYTIP_SCHEMA_PATH=/usr/local/share/dailytipdraft/schema.sql \
DAILYTIP_TEMPLATE_DIR=/usr/local/share/dailytipdraft/templates \
dailytipdraft
```

## Deployment

### systemd

Use the installer on a Linux host with systemd:

```bash
sudo ./install.sh
```

The installer builds `target/release/dailytipdraft`, installs the binary to `/usr/local/bin/dailytipdraft`, installs `schema.sql` and templates to `/usr/local/share/dailytipdraft`, creates a `dailytipdraft` system user, stores runtime data in `/var/lib/dailytipdraft`, and starts `dailytipdraft.service`.

Useful commands:

```bash
sudo systemctl status dailytipdraft
sudo journalctl -u dailytipdraft -f
sudo systemctl restart dailytipdraft
sudo ./install.sh uninstall
```

Set a different loopback port during install when needed:

```bash
sudo BIND_ADDR=127.0.0.1:3010 ./install.sh
```

The generated admin token is printed in the service logs on first startup:

```bash
sudo journalctl -u dailytipdraft -n 100 --no-pager
```

### Docker

Build and run:

```bash
docker build -t dailytipdraft .
docker run -d \
  --name dailytipdraft \
  --network host \
  -v dailytipdraft-data:/var/lib/dailytipdraft \
  dailytipdraft
```

Read the first-start admin token:

```bash
docker logs dailytipdraft
```

The Docker image listens on `127.0.0.1:3001` by default and stores `settings.yaml` plus `dailytip.db` in `/var/lib/dailytipdraft`.

## API Documentation

The API reference is split by audience:

- [Client API](docs/client-api.md): protobuf routes for `/tips`, `/topics`, and `/review`.
- [Admin API](docs/admin-api.md): JSON/session routes for settings, key management, topics, tipcards, and the root browser app JSON routes.
- [Agent Server Talk Guide](docs/agent-server-guide.md): operational playbook for agents that need to talk with a running server.

## Database Schema

| Table | Purpose |
|---|---|
| `api_keys` | Hashed client keys with display names |
| `topic_classes` | Topic class definitions, including card behavior type |
| `topics` | Topic categories linked to topic classes |
| `tipcards` | Generated tips (full + compressed content) with card type |
| `review_states` | Per-card review state, status, and next review timestamp |

Tip content can include markdown such as headings, lists, emphasis, links, blockquotes, inline code, and fenced code blocks. The root app and legacy admin dashboard render that markdown client-side after escaping unsafe HTML. Protobuf and JSON API routes still return raw `full_content` and `compressed_content` strings so external clients can choose their own renderer.

## Running Tests

```bash
cargo test
```

The test suite spawns a real server on an ephemeral port for each test group and exercises auth, settings CRUD, key management, the full tips→review flow, and error handling.
Tests use isolated temporary settings files, so running them does not overwrite your local `settings.yaml`.

> **Note:** The `test_full_api_flow` test uses the missing-key fallback and does not call a real LLM endpoint by default.
