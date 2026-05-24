# Denpie

A Rust-based backend service that generates, serves, and schedules daily tip cards using SM-2 review scheduling. LLM content generation is powered by any OpenAI-compatible API endpoint (Gemini, OpenRouter, etc.) via `async-openai`.

## Features

- **Intelligent Scheduling**: SM-2 review intervals optimize tip delivery based on user review grades.
- **Casual Cards**: Queue-style tips can be dismissed or acknowledged so clients can pull the next card immediately; acknowledged cards are scheduled.
- **Repeatable Cards**: Re:word-style cards can be dismissed, repeated, memorized, or acknowledged; clients can advance after repeatable review actions, and repeated cards come back when due through scheduling.
- **Card Types**: Topics use a card behavior type: casual, repeatable, manual, or custom.
- **Daily Topic Cards**: Each topic/type returns a configurable number of stable cards, with per-topic overrides for count and card refresh time. A server-side daily worker adds fresh generated cards at the configured window without requiring the browser dashboard to be open. Current cards stay active unless the user reviews or deletes them.
- **Forced Card Refresh**: The settings screen and protobuf API use the same backend refresh path as the daily worker: pull one fresh generated card for every generated topic, or target selected generated topics, while keeping current cards untouched.
- **Pinned Tipcards**: Any active card can be pinned from the control panel or API so it stays visible in a separate top section until unpinned.
- **Topic Icons**: Each topic gets an Iconify icon picked from a curated allowlist (`config/topic_icons.json`) when first created; the dashboard lets you click a topic icon to ask the LLM for a new pick and reroll its accent color. Cards and the dashboard show that icon with the stored HSL color (initial color is derived from the topic name). Missing or failed picks fall back to a neutral default (`lucide:tag`).
- **Tipcard Images**: Manual cards can be saved with attached images, and existing cards can receive or clear image attachments from the browser control panel.
- **Fast Unified Flow**: The dashboard flow uses cursor-loaded card summaries, a measured virtual grid, on-demand full-card details, and protected file-backed image URLs so large card sets keep scrolling smoothly. Sort cards alphabetically by topic (default), by newest-first date, or a saved drag order; the chosen sort is remembered across visits. Dragging unpinned cards switches to drag order for the main flow; dragging pinned cards only reorders the pinned top section and leaves the active date/topic sort unchanged.
- **Archive Search**: The browser archive can search card titles, topics, full text, compressed text, classes, and statuses, with status filters for active, acknowledged, memorized, dismissed, and custom cards. Sort cards alphabetically by topic name (default) or newest-first date; the chosen sort is remembered across visits.
- **Custom Tipcards**: External workflows can submit grey `custom_tip` cards for summaries or reminders without adding scheduling review state.
- **Active Card Limit**: A per-user max-active-cards setting can stop new card creation while still allowing due and pinned cards to be reviewed.
- **Any OpenAI-Compatible LLM**: Configure each user's API key, base URL, and model through the protobuf API or browser dashboard — no hardcoded vendor lock-in.
- **Token Spend Counters**: The browser dashboard tracks OpenAI-compatible `usage.total_tokens` for the current user's daily, monthly, and lifetime LLM calls.
- **Unified Protobuf API**: `POST /api` manages tips, reviews, settings, keys, topics, topic deletion, card pinning, cards, and summary counts. The API key owner determines the data scope.
- **Root Control Page**: `/` serves a shadcn-inspired Yew/WebAssembly browser control panel with direct Radix icons that talks to the same protobuf API, with readable shadcn dark destructive controls, compact mobile stats, stable per-card loading skeletons, compact/full card text controls, searchable archive filters with topic/date sort toggles, topic/date/drag-order sort toggles in Unified Flow, a remembered grid/column flow layout switch, readable-width list expansion, syntax-highlighted markdown code blocks, solid Unified Flow cards when more than eight cards are visible, title-row fullscreen card viewing that hides background cards, old-template-compatible tipcard actions, and touch-friendly handle-based card reordering with edge auto-scroll.
- **Mobile-Friendly Asset Delivery**: The server compresses frontend assets, sends long-lived cache headers for hashed JavaScript/WebAssembly and static files, registers a service worker for repeat visits, and revalidates private tipcard images with ETags. The mobile control page avoids fixed background and backdrop-filter work on small screens.
- **Single Dashboard Surface**: The browser dashboard is served only at `/`;
- **CSS-Only Motion**: The control page uses fast page-entry, card-entry, and compact-to-full tipcard animations with reduced-motion support.
- **Markdown Tipcards**: API responses keep the original raw markdown-capable text so clients can render it however they need.
- **Optional Server Self-Updates**: Disabled by default. The systemd install includes a root-owned updater timer; enabling it through the API polls GitHub, rebuilds from the configured repository branch, installs the new binary, schema, frontend assets, and static assets, and restarts the service.
- **Bootstrap Admin Token**: When admin setup is still required, the server generates and prints an admin token. Use it only to create the first admin user and, after setup, to bootstrap an API key for that admin user.
- **Protobuf API**: The only public API is a single protobuf request/response envelope for both client and admin operations.
- **Multi-User, Multi-Client**: Each user has isolated topics, cards, review state, LLM settings, token spend, and API keys. A user's scheduling state is shared across that user's clients (desktop widget, Telegram bot, etc.).
- **SQLite Database**: Lightweight persistence via `sqlx` with bound query parameters.

## Screenshots

| Dashboard | Unified Flow | Fullscreen Card |
| :---: | :---: | :---: |
| ![Dashboard](docs/assets/dashboard.png) | ![Unified Flow](docs/assets/unified-flow.png) | ![Fullscreen Card](docs/assets/fullscreen-card.png) |

## Tech Stack

| Layer | Technology |
|---|---|
| Language | Rust (edition 2024) |
| Web Framework | Axum |
| Database | SQLite (via SQLx) |
| Async Runtime | Tokio |
| LLM Client | `async-openai` |
| Serialization | Protocol Buffers (`prost`) |
| Frontend | Static HTML control panel at `/` using Tailwind plus shadcn-style design tokens |
| Public API | Protocol Buffers over HTTP |

## Project Structure

```
.
├── src/
│   ├── main.rs        # Router setup, state, app initialization
│   ├── api.rs         # Public API module exports
│   ├── api/           # Protobuf transport, request types, tip generation, and admin helpers
│   ├── auth.rs        # HTTP session transport and API key middleware wrapper
│   ├── config/        # Typed settings and YAML load/save/update store
│   ├── db/            # Repository modules for SQL-backed persistence
│   ├── domain/        # Pure scheduling, review, and tipcard rules
│   ├── error.rs       # Shared application error type and HTTP mapping
│   ├── services/      # Settings and API key service orchestration
│   ├── autoupdate.rs  # Optional in-process GitHub change watcher
│   ├── llm.rs         # LLM wrappers (generate_new_card, compress_card, generate_card_title)
│   └── scheduling/   # Scheduling algorithms, currently SM-2
├── migrations/        # SQL schema snapshots for database setup
├── schema.sql         # SQLite schema reference kept for installs/tests
├── proto/
│   └── denpie.proto # Protobuf schema for the unified API
├── frontend/          # Yew/WebAssembly browser dashboard
├── static/
│   └── assets/        # Browser dashboard images and static files
├── data/tipcard-images # Runtime dashboard tipcard image files when DENPIE_DATA_DIR=data
├── docs/              # API documentation
└── settings.yaml      # Runtime config, generated locally and ignored
```

## Getting Started

### Prerequisites

- Rust 1.95.0 or newer (latest stable)
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
   In debug builds, `cargo run` automatically runs `trunk build` (debug profile) for the Yew frontend before the server starts, and skips the rebuild when `frontend/dist` is already up to date. Install the frontend target and Trunk first with `rustup target add wasm32-unknown-unknown` and `cargo install trunk --locked`. The frontend `Trunk.toml` disables Trunk's JavaScript minifier because older Trunk versions cannot parse current wasm-bindgen output cleanly. Set `DENPIE_SKIP_FRONTEND_BUILD=1` to skip this automatic frontend build, or set `DENPIE_FRONTEND_DIST` when serving a prebuilt frontend from another directory. Production installs and Docker still use `trunk build --release`.
   The server starts on `http://127.0.0.1:3017` by default. On the first run it will:
   - Create `denpie.db` and apply `schema.sql` automatically.
   - Create the tipcard image directory. By default this is `tipcard-images` inside `DENPIE_DATA_DIR`; set `DENPIE_IMAGE_DIR` to store dashboard image files elsewhere.
   - Generate and print a setup admin token to the console only if admin setup is still required.

4. **Create the first admin user** from the root page at `http://127.0.0.1:3017/`. Enter a username, password, and the printed setup token, then click **Create Admin User**. Existing single-user data, settings, and API keys from an older database are assigned to this first setup user.

5. **Create an API key** from the browser dashboard, or call `POST /api` with `bootstrap_api_key` after setup. `bootstrap_api_key` uses the printed `admin_token` and creates a key owned by the first admin user.

6. **Configure your LLM** through the browser dashboard or `POST /api` with `update_settings`:
   - **LLM Model** — e.g. `google/gemini-2.5-pro` or `openai/gpt-4o`
   - **LLM API Key** — your provider API key
   - **LLM Base URL** — e.g. `https://openrouter.ai/api/v1` or `https://generativelanguage.googleapis.com/v1beta/openai`
   - **Prompt Template** — use `{topic}` as the placeholder; `{context}`, `{existing_cards}`, and `{dismissed_cards}` can place prior card titles explicitly

   Browser settings changes autosave after a short debounce. **Save Now** forces the pending settings patch to be written immediately.

   Each topic can also define its own prompt, daily card count, and card refresh time with `update_topic`; empty topic time fields fall back to the current user's default time zone setting. Topics can be deleted from the browser control panel or with `delete_topic`; deleting a topic also deletes its cards and review state for that user. The browser control panel has a type switcher for Casual, Repeatable, and Manual cards; Manual cards are saved from user-entered text and do not call the LLM. In Manual mode, Tab moves from the topic field straight to the manual card content textarea, and in the textarea, Shift+Enter saves/adds the card. External systems can use `submit_custom_tipcard` to store `custom_tip` cards for summaries and reminders; these cards return `tipcard_type = "custom_tip"` and do not create scheduling review state. Active cards can be pinned so they remain in a separate top section even before their next review time; unpinning returns them to normal scheduling. Daily refresh runs inside the server once per topic window and uses the same behavior as **Force Daily Refresh**: it adds fresh generated cards without moving, dismissing, or rescheduling current cards. Protobuf clients can call `force_daily_refresh` for generated topic targeting. Set **Max Active Cards** to stop creating new cards once your active review state reaches that number; `0` means unlimited, and existing due/pinned cards remain available. When a new generated card is created, the server sends generated titles from existing and dismissed cards for the same topic/type so the model can avoid repeats.

7. **Use the API key** in `ApiRequest.auth` for every operation except `bootstrap_api_key`. All reads and writes are scoped to the user that owns the key.

## User Model

Denpie stores all users in the same SQLite database, but user data is isolated by `user_id`. Topics, tipcards, review states, API keys, token usage, and user settings are all scoped to the authenticated user. Topic names only need to be unique within one user account, so two users can both have a `rust` topic without sharing cards.

Browser authentication uses username/password sessions. API authentication stays wire-compatible: clients continue to send an API key in `ApiRequest.auth`, and the server resolves the key owner internally. The setup token is only for bootstrap; normal browser login uses the username and password.

## Configuration (`settings.yaml`)

Global server bootstrap and autoupdate configuration lives in `settings.yaml`. Per-user LLM, prompt, UI, daily schedule, and active-card-limit settings live in SQLite. **Do not commit `settings.yaml`** — it contains the setup token and server update settings.

| Key | Description | Default |
|---|---|---|
| `admin_token` | Setup token for creating the first admin user and bootstrapping an admin-owned API key | Auto-generated on first run |
| `autoupdate_enabled` | Enable GitHub commit polling and server self-updates | `false` |
| `autoupdate_repo` | GitHub repository in `owner/repo` form, or a GitHub URL | `slopfire/denpie` |
| `autoupdate_branch` | Branch or ref checked through the GitHub commits API | `master` |
| `autoupdate_check_interval_secs` | Poll interval in seconds; values below 60 are clamped to 60 | `3600` |
| `autoupdate_command` | Optional local shell command for non-systemd server updates after a new commit is detected | *(empty)* |
| `autoupdate_last_seen_sha` | Last GitHub commit SHA recorded by the updater | *(empty)* |

Per-user settings stored in SQLite include `llm_model`, `llm_compress_model`, `prompt_template`, `llm_api_key`, LLM base URLs, reasoning/compression settings, browser theme/blur choices, `daily_time_zone`, `daily_update_time`, and `max_active_cards`.

### Server Self-Updates

Server self-updates are intentionally off by default. For the systemd installation, the installer enables a `denpie-autoupdate.timer` that reads `settings.yaml`; checking **Enable Server Self-Updates** in the app is enough. On the first successful check the updater records the current commit SHA as a baseline and does not update. On later checks, a changed SHA triggers a root-owned update helper that fetches the configured branch, runs `trunk build --release` for the frontend and `cargo build --release --package denpie` for the server, installs the new binary plus shared files, records the new SHA, and restarts `denpie.service`. Each updater step writes status to `/admin/autoupdate/status`, and long network, build, install, and restart steps have timeouts so a stalled helper becomes a visible failure instead of leaving stale progress. The helper trusts only its configured checkout path for Git operations, so operators should not need to add a global `safe.directory` exception. The host must keep the build tools available after installation (`git`, `cargo` from the installer-managed rustup toolchain or another Rust installation, and `protoc`/`protobuf-compiler`).

The settings screen also has a **Check Server Now** button. It shows staged progress while saving server update settings, checking GitHub, and, when a new commit is found, starting the server update flow. If `autoupdate_command` is empty, it starts the default systemd updater service (`denpie-autoupdate.service`) with `systemctl start --no-block`; set `DENPIE_AUTOUPDATE_SERVICE` to use a different unit name. The installer also installs a narrow polkit rule allowing the `denpie` service user to start only that updater unit. If the unit or permission rule is missing, the check fails with a configuration message instead of a raw systemctl failure. For non-systemd or custom deployments it runs the configured `autoupdate_command` immediately after checking GitHub. When a server update is found, the browser shows the target commit and a persisted updater log from `/admin/autoupdate/status`, so progress survives page reloads while active updater phases keep polling. The updater records `autoupdate_last_seen_sha` only after a successful service restart, allowing failed restarts to be retried. You can also manually trigger the root-owned updater with:

```bash
sudo systemctl start denpie-autoupdate.service
```

Manual systemd starts bypass the saved check interval; the helper still exits without changes when the recorded SHA already matches the remote branch.

Default repository comes from this repo's `origin` remote: `slopfire/denpie`. You can override it with another `owner/repo`, `https://github.com/owner/repo`, or `git@github.com:owner/repo.git` value. Example:

```yaml
autoupdate_enabled: true
autoupdate_repo: slopfire/denpie
autoupdate_branch: master
autoupdate_check_interval_secs: 1800
```

For non-systemd or custom deployments, set `autoupdate_command` to a local command that performs the server update. In that mode, scheduled checks and the **Check Server Now** button run the command with the same user, permissions, and working directory as the server process; if it succeeds, the server exits with code `75` so an external supervisor can restart it.

## Runtime Environment

The server can run from the project directory with defaults, or from an installed location with explicit paths.

| Variable | Description | Default |
|---|---|---|
| `DENPIE_BIND_ADDR` | Listen address and port | `127.0.0.1:3017` |
| `DENPIE_RP_ID` | WebAuthn relying party ID for passkeys. Must be the public site host or a registrable parent domain of it. | host from `DENPIE_RP_ORIGIN` |
| `DENPIE_RP_ORIGIN` | Public HTTPS origin used for passkey registration and login. | `http://localhost:3017` |
| `DENPIE_DATA_DIR` | Directory for `settings.yaml` and `denpie.db` | current directory |
| `DENPIE_SCHEMA_PATH` | Path to `schema.sql` | `schema.sql` in the current directory |
| `DENPIE_FRONTEND_DIST` | Directory containing the built Yew frontend | `frontend/dist` in the current directory |
| `DENPIE_STATIC_DIR` | Directory served at `/static` | `static` in the current directory |

Example:

```bash
DENPIE_BIND_ADDR=127.0.0.1:3017 \
DENPIE_RP_ID=denpie.com \
DENPIE_RP_ORIGIN=https://denpie.com \
DENPIE_DATA_DIR=/var/lib/denpie \
DENPIE_SCHEMA_PATH=/usr/local/share/denpie/schema.sql \
DENPIE_FRONTEND_DIST=/usr/local/share/denpie/frontend/dist \
DENPIE_STATIC_DIR=/usr/local/share/denpie/static \
denpie
```

## Deployment

### systemd

Use the installer on a Linux host with systemd:

```bash
./install.sh
```

The installer installs Rust with rustup if `cargo` is not available, builds the release Yew frontend with Trunk, builds `target/release/denpie`, installs the binary to `/usr/local/bin/denpie`, installs `schema.sql`, frontend assets, and static assets to `/usr/local/share/denpie`, creates a `denpie` system user, repairs `/var/lib/denpie` ownership for that service user, and restarts `denpie.service` so an existing installation starts using the new files immediately. It uses `sudo` internally for system directories, service users, and systemd commands.
It also installs, enables, and restarts `denpie-autoupdate.timer`, which stays idle unless `autoupdate_enabled: true` is set in `settings.yaml`. The updater helper defaults to a 120 second timeout for network and restart steps, 1800 seconds for builds, and 300 seconds for installing files; override them with `NETWORK_TIMEOUT_SECS`, `BUILD_TIMEOUT_SECS`, or `INSTALL_TIMEOUT_SECS` when running `install.sh`.

Useful commands:

```bash
sudo systemctl status denpie
sudo journalctl -u denpie -f
sudo systemctl restart denpie
sudo systemctl status denpie-autoupdate.timer
sudo systemctl start denpie-autoupdate.service
./install.sh uninstall
```

Set a different loopback port during install when needed:

```bash
BIND_ADDR=127.0.0.1:3010 ./install.sh
```

Set a different public passkey domain during install when needed:

```bash
RP_ID=example.com RP_ORIGIN=https://example.com ./install.sh
```

The generated admin token is printed in the service logs on first startup:

```bash
sudo journalctl -u denpie -n 100 --no-pager
```

### Docker

Build and run:

```bash
docker build -t denpie .
docker run -d \
  --name denpie \
  --network host \
  -v denpie-data:/var/lib/denpie \
  denpie
```

Read the first-start admin token:

```bash
docker logs denpie
```

The Docker image listens on `127.0.0.1:3017` by default and stores `settings.yaml` plus `denpie.db` in `/var/lib/denpie`.

## API Documentation

The unified protobuf API is the only public API:

- [Unified Protobuf API](docs/protobuf-api.md): canonical `POST /api` surface for both client and admin operations.
- [Agent Server Talk Guide](docs/agent-server-guide.md): operational playbook for agents that need to talk with a running server.

## Database Schema

| Table | Purpose |
|---|---|
| `api_keys` | Hashed client keys with display names |
| `users` | Login profiles, roles, display names, and avatar data |
| `topics` | Topic categories scoped to users, with card type, optional prompt template, daily refresh overrides, Iconify icon id, and color hue |
| `tipcards` | Generated, manual, and custom tips with full content, compact compressed content, title, card type, and pin state |
| `review_states` | Per-card review state, status, and next review timestamp |
| `tipcard_images` | File-backed image attachment metadata for cards |
| `llm_token_usage` | Token usage returned by LLM calls, used for daily, monthly, and total dashboard counters |
| `user_settings` | Per-user LLM, prompt, UI, schedule, and active-card-limit settings |
| `daily_refresh_runs` | Per-topic daily refresh windows already processed by the worker |
| `passkeys` | WebAuthn passkeys for browser login |

Topic rows can override the daily refresh defaults with `daily_card_count` and `daily_update_time` in the browser dashboard; the global `daily_time_zone` setting is used for refresh windows. Topics can also override `llm_compression_level` with their own compression preset. Empty time and compression fields inherit the global settings; empty or zero count falls back to one card.

The token counters use `usage.total_tokens` from each OpenAI-compatible chat completion response. Providers that omit usage metadata contribute zero tokens for that call.

Tip content can include markdown such as headings, lists, emphasis, links, blockquotes, inline code, and fenced code blocks. The built-in browser UI supports common inline combinations such as bold text containing inline code, and fenced blocks can use language tags like `` ```rust ``, `` ```bash ``, or `` ```json `` for browser-side Shiki syntax highlighting with copy buttons. Unknown or missing fenced-code languages render as escaped plaintext. Fenced code blocks with more than five lines collapse in compact card view and expand when the card text is expanded or fullscreen. The protobuf API returns raw `full_content` and `compressed_content` strings so clients can choose their own renderer. Generated cards below roughly 420 characters or 70 words skip LLM compression and reuse the full content as the compact card text. Longer generated cards use the configured compression preset: `light` keeps more context, `balanced` is the default compact card, `strong` trims to essentials, and `ultra` creates reminder-sized cards. During compression, fenced code blocks are preserved verbatim while only surrounding prose is compacted.

## Running Tests

```bash
cargo test
```

The test suite spawns a real server on an ephemeral port for each test group and exercises bootstrap auth, settings CRUD, key management, the full tips→review flow, and error handling through `POST /api`.
Tests use isolated temporary settings files, so running them does not overwrite your local `settings.yaml`.

> **Note:** The `test_full_api_flow` test uses the missing-key fallback and does not call a real LLM endpoint by default.

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
