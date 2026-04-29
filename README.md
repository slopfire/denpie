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
- **Optional GitHub Autoupdate**: Disabled by default. The systemd install includes a root-owned updater timer, so enabling the checkbox is enough to poll GitHub, rebuild from the configured repository branch, install the new binary/templates/schema, and restart the service.
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
│   ├── autoupdate.rs  # Optional in-process GitHub change watcher
│   ├── dashboard.rs   # Admin HTML page + settings/key management REST endpoints
│   ├── llm.rs         # LLM wrappers (generate_new_card, compress_card, generate_card_title)
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

4. **Open the browser client app** at `http://127.0.0.1:3001/` and log in with the printed token. The app includes the dashboard, unified flow over due active cards, compact/expandable and copyable card text, drag-and-drop card ordering, card deletion, grid/column layout switching, per-topic prompt overrides, settings, API keys, archive, and a color scheme selector with Default, Ayu, Solarized, Dracula, and Slate themes. Adding, dismissing, repeatable memorizing/repeating, and daily casual-card refreshes show skeleton loading cards while the server generates replacements without reusing cards already visible in the flow.

   The legacy admin dashboard remains available at `http://127.0.0.1:3001/admin`.

5. **Configure your LLM** in the Configuration panel:
   - **LLM Model** — e.g. `google/gemini-2.5-pro` or `openai/gpt-4o`
   - **LLM API Key** — your provider API key
   - **LLM Base URL** — e.g. `https://openrouter.ai/api/v1` or `https://generativelanguage.googleapis.com/v1beta/openai`
   - **Prompt Template** — use `{topic}` as the placeholder; `{context}`, `{existing_cards}`, and `{dismissed_cards}` can place prior card titles explicitly

   Each topic can also define its own prompt in the Active Topics panel. Empty topic prompts fall back to the global prompt template. When a new card is generated, the server sends generated titles from existing and dismissed cards for the same topic/type so the model can avoid repeats.

6. **Generate a client API key** in the Access Keys panel. Pass it as the `Authorization` header on every `/tips` and `/review` request.

## Configuration (`settings.yaml`)

All runtime configuration lives in `settings.yaml` and is managed exclusively through the dashboard. **Do not commit this file** — it contains your API key and admin token.

| Key | Description | Default |
|---|---|---|
| `admin_token` | Hashed token for dashboard login | Auto-generated on first run |
| `llm_model` | Model identifier string | `google/gemini-3.1-flash` |
| `llm_compress_model` | Model identifier string for compressed summaries and generated card titles | `google/gemini-3.1-flash-lite-preview` |
| `llm_reasoning_effort` | Reasoning effort for generated tips: `none`, `minimal`, `low`, `medium`, `high`, or `xhigh` | `none` |
| `llm_compress_reasoning_effort` | Reasoning effort for compressed summaries and generated titles; `none` disables compression/title thinking tokens | `none` |
| `prompt_template` | Tip generation prompt (`{topic}` placeholder) | `Give a smart tip about {topic}.` |
| `llm_api_key` | API key for the LLM provider | *(empty — set via dashboard)* |
| `llm_base_url` | Base URL for the OpenAI-compatible API | `https://openrouter.ai/api/v1` |
| `llm_compress_base_url` | Base URL for compression requests; defaults to `llm_base_url` when missing | `https://openrouter.ai/api/v1` |
| `color_scheme` | Browser client color scheme | `default` |
| `autoupdate_enabled` | Enable GitHub commit polling and command-based updates | `false` |
| `autoupdate_repo` | GitHub repository in `owner/repo` form, or a GitHub URL | `slopfire/dailytipdraft` |
| `autoupdate_branch` | Branch or ref checked through the GitHub commits API | `main` |
| `autoupdate_check_interval_secs` | Poll interval in seconds; values below 60 are clamped to 60 | `3600` |
| `autoupdate_command` | Optional local shell command for non-systemd installs after a new commit is detected | *(empty)* |
| `autoupdate_last_seen_sha` | Last GitHub commit SHA recorded by the updater | *(empty)* |

### GitHub Autoupdate

Autoupdate is intentionally off by default. For the systemd installation, the installer enables a `dailytipdraft-autoupdate.timer` that reads `settings.yaml`; checking **Enable GitHub autoupdate** in the app is enough. On the first successful check the updater records the current commit SHA as a baseline and does not update. On later checks, a changed SHA triggers a root-owned update helper that fetches the configured branch, runs `cargo build --release`, installs the new binary plus shared files, records the new SHA, and restarts `dailytipdraft.service`. The host must keep the build tools available after installation (`git`, `cargo`, and `protoc`/`protobuf-compiler`).

Default repository comes from this repo's `origin` remote: `slopfire/dailytipdraft`. You can override it with another `owner/repo`, `https://github.com/owner/repo`, or `git@github.com:owner/repo.git` value. Example:

```yaml
autoupdate_enabled: true
autoupdate_repo: slopfire/dailytipdraft
autoupdate_branch: main
autoupdate_check_interval_secs: 1800
```

For non-systemd or custom deployments, set `autoupdate_command` to a local command that performs the update. In that mode, the command runs with the same user, permissions, and working directory as the server process; if it succeeds, the server exits with code `75` so an external supervisor can restart it.

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
It also installs and enables `dailytipdraft-autoupdate.timer`, which stays idle unless `autoupdate_enabled: true` is set in `settings.yaml`.

Useful commands:

```bash
sudo systemctl status dailytipdraft
sudo journalctl -u dailytipdraft -f
sudo systemctl restart dailytipdraft
sudo systemctl status dailytipdraft-autoupdate.timer
sudo systemctl start dailytipdraft-autoupdate.service
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
| `topics` | Topic categories linked to topic classes, with optional per-topic prompt template |
| `tipcards` | Generated tips with full content, compact compressed content, title, and card type |
| `review_states` | Per-card review state, status, and next review timestamp |

Tip content can include markdown such as headings, lists, emphasis, links, blockquotes, inline code, and fenced code blocks. The root app and legacy admin dashboard render that markdown client-side after escaping unsafe HTML. Protobuf and JSON API routes still return raw `full_content` and `compressed_content` strings so external clients can choose their own renderer.

## Running Tests

```bash
cargo test
```

The test suite spawns a real server on an ephemeral port for each test group and exercises auth, settings CRUD, key management, the full tips→review flow, and error handling.
Tests use isolated temporary settings files, so running them does not overwrite your local `settings.yaml`.

> **Note:** The `test_full_api_flow` test uses the missing-key fallback and does not call a real LLM endpoint by default.
