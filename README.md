# Denpie

A Rust/Axum backend that generates, serves, and schedules daily tip cards with SM-2 review scheduling. Content generation uses any OpenAI-compatible endpoint (Gemini, OpenRouter, etc.) via `async-openai`.

## Features

- **SM-2 scheduling** with review grades, due windows, and repeatable/casual/manual/custom card types.
- **Daily topic cards** — server-side worker refreshes generated cards once per topic window; current cards stay active.
- **Manual & custom cards** — user-entered manual cards without LLM calls; external `custom_tip` cards with no review state.
- **Topic icons** — LLM-picked Iconify icon from `config/topic_icons.json` with HSL accent color; fallback `lucide:tag`.
- **Tipcard images** — browser-compressed uploads; server rejects >10 MB decoded and recompresses >800 KB with libcaesium.
- **Pinned cards** — active cards can be pinned to stay visible ahead of schedule.
- **Active card limit** — caps new generated/manual cards while still serving due and pinned cards.
- **Token spend counters** — per-user daily/monthly/lifetime `usage.total_tokens`.
- **Multi-user, multi-client** — topics, cards, review state, settings, and keys are isolated by user.
- **Single protobuf API** — `POST /api` is the public surface; `/` is the browser dashboard.
- **Optional server self-updates** — GitHub polling with systemd helper (disabled by default).

## Screenshots

| Dashboard | Unified Flow | Fullscreen Card |
| :---: | :---: | :---: |
| ![Dashboard](docs/assets/dashboard.png) | ![Unified Flow](docs/assets/unified-flow.png) | ![Fullscreen Card](docs/assets/fullscreen-card.png) |

## Tech Stack

| Layer | Technology |
|---|---|
| Language | Rust (edition 2024) |
| Web Framework | Axum |
| Database | SQLite via SQLx |
| Async Runtime | Tokio |
| LLM Client | `async-openai` + shared `reqwest` transport |
| Serialization | Protocol Buffers (`prost`) |
| Frontend | Yew/WebAssembly + Tailwind |
| Public API | Protobuf over HTTP (`POST /api`) |

## Project Structure

```
.
├── src/
│   ├── main.rs              # entry point, migrations, server start
│   ├── app.rs               # Axum router, state, middleware
│   ├── auth.rs              # session + API key auth
│   ├── error.rs             # AppError / AppResult
│   ├── types.rs             # shared request/response types
│   ├── http_client.rs       # shared reqwest::Client
│   ├── daily_refresh.rs     # scheduled daily refresh worker
│   ├── image_compress.rs    # image compression helpers
│   ├── image_store.rs       # tipcard image file storage
│   ├── context.rs           # prompt context rendering
│   ├── api.rs / api/        # protobuf transport + thin handlers
│   ├── auth.rs              # session/API-key middleware
│   ├── config/              # YAML settings, topic icons, WebAuthn
│   ├── db/                  # migrations + repositories
│   ├── db/repositories/     # SQL modules (tipcards split into submodules)
│   ├── domain/              # pure scheduling/review/tipcard rules
│   ├── scheduling/          # SM-2 implementation
│   ├── services/            # business orchestration (api_keys, autoupdate,
│   │                        # review, settings, tipcards, tips, topics)
│   ├── autoupdate/          # self-update config, runner, systemd helper
│   ├── llm/                 # transport, cards, icons, markdown, compression
│   ├── dashboard/           # browser dashboard handlers/response/util
│   └── tests/               # integration tests (api_flow, auth, review,
│                            # server, settings, tipcard)
├── proto/denpie.proto       # canonical protobuf schema
├── schema.sql               # SQLite schema
├── migrations/              # schema snapshots
├── frontend/                # Yew dashboard
├── static/                  # static assets
└── settings.yaml            # generated local config (do not commit)
```

## Getting Started

With Nix:

```bash
just shell       # or nix-shell
just setup       # verify toolchain
just dev         # backend + frontend watchers
```

Without Nix, you need Rust 1.95.0, `wasm32-unknown-unknown`, Trunk, `protoc`, and SQLite. The repo's `rust-toolchain.toml` selects the right Rust version.

### First run

```bash
cargo run
```

This creates `denpie.db`, applies `schema.sql`, builds the frontend if needed, and prints a one-time `admin_token`.

1. Open `http://127.0.0.1:3017/` and create the first admin user with the printed token.
2. Create an API key via the dashboard or call `bootstrap_api_key` with the `admin_token`.
3. Set the user's LLM model, API key, base URL, and prompt template through the dashboard or `update_settings`.
4. Use the API key in `ApiRequest.auth` for every `POST /api` call.

Use `DENPIE_SKIP_FRONTEND_BUILD=1` to skip the frontend build during backend work.

## Development Workflow

```bash
just check   # cargo check, no frontend rebuild
just test    # Rust test suite
just ci      # fmt + clippy + tests + release frontend build
```

Debug logging uses `tracing`:

```bash
RUST_LOG=denpie=debug just backend
```

## Configuration

Global bootstrap settings live in `settings.yaml` (generated, do not commit):

| Key | Description | Default |
|---|---|---|
| `admin_token` | First-user setup token | auto-generated |
| `autoupdate_enabled` | GitHub polling / self-updates | `false` |
| `autoupdate_repo` | `owner/repo` or GitHub URL | `slopfire/denpie` |
| `autoupdate_branch` | Branch to watch | `master` |
| `autoupdate_check_interval_secs` | Poll interval (min 60) | `3600` |
| `autoupdate_command` | Non-systemd update command | *(empty)* |
| `autoupdate_last_seen_sha` | Last seen remote SHA | *(empty)* |

Per-user settings (SQLite): LLM model/compress model/base URL/API key, prompt template, reasoning/compression presets, theme, `daily_time_zone`, `daily_update_time`, `max_active_cards`.

### Server self-updates

Enabled via `autoupdate_enabled`. The systemd helper rebuilds frontend + backend, installs files, records the new SHA, and restarts `denpie.service`. Status is written to `/admin/autoupdate/status`. Default helper timeouts: network/restart 120s, build 1800s, install 300s.

## Runtime Environment

| Variable | Description | Default |
|---|---|---|
| `DENPIE_BIND_ADDR` | Listen address | `127.0.0.1:3017` |
| `DENPIE_RP_ID` | WebAuthn RP ID | derived from `DENPIE_RP_ORIGIN` |
| `DENPIE_RP_ORIGIN` | Public origin for passkeys | `http://localhost:3017` |
| `DENPIE_RP_EXTRA_ORIGINS` | Extra allowed origins | none |
| `DENPIE_PROD` | Force `Secure` cookies | off (auto-on for `https`) |
| `DENPIE_DATA_DIR` | `settings.yaml` + `denpie.db` | current directory |
| `DENPIE_SCHEMA_PATH` | Path to `schema.sql` | `./schema.sql` |
| `DENPIE_FRONTEND_DIST` | Built frontend dir | `./frontend/dist` |
| `DENPIE_STATIC_DIR` | Static assets dir | `./static` |
| `DENPIE_IMAGE_DIR` | Tipcard image dir | `$DENPIE_DATA_DIR/tipcard-images` |

## Deployment

### systemd

```bash
./install.sh
```

Installs binary, schema, frontend, static assets, creates the `denpie` user, enables `denpie-autoupdate.timer`, and restarts the service. Override port or passkey domain:

```bash
BIND_ADDR=127.0.0.1:3010 RP_ID=example.com RP_ORIGIN=https://example.com ./install.sh
```

The first-start admin token is in the service log:

```bash
sudo journalctl -u denpie -n 100 --no-pager
```

### Docker

```bash
docker build -t denpie .
docker run -d --name denpie --network host \
  -e DENPIE_RP_ORIGIN=https://denpie.example.com \
  -e DENPIE_RP_ID=denpie.example.com \
  -v denpie-data:/var/lib/denpie \
  denpie
```

Set `DENPIE_UID`/`DENPIE_GID` to match the host data directory owner, and `DENPIE_BIND_ADDR=0.0.0.0:3017` when behind a reverse proxy.

### DockerHub CI/CD

Repository secrets:

- `DOCKERHUB_USERNAME`
- `DOCKERHUB_TOKEN`
- Optional: `DOCKERHUB_REPOSITORY`

Tags: branch, Git tag, `sha-<commit>`, `latest`.

## API Documentation

- [`docs/protobuf-api.md`](docs/protobuf-api.md) — canonical `POST /api` surface.
- [`docs/agent-server-guide.md`](docs/agent-server-guide.md) — quick agent operations.
- [`docs/feature-integration.md`](docs/feature-integration.md) — where to add new code.

## Database Schema

| Table | Purpose |
|---|---|
| `api_keys` | SHA-256 hashed client keys |
| `users` | Profiles, roles, avatars |
| `topics` | Per-user topics with type, prompt override, icon, color, daily overrides |
| `tipcards` | Generated/manual/custom cards with content, title, pin state |
| `review_states` | Per-card SM-2 state, status, `repeats`, next review time |
| `tipcard_images` | Image attachment metadata |
| `llm_token_usage` | Per-call token totals |
| `user_settings` | Per-user LLM/UI/schedule settings |
| `daily_refresh_runs` | Per-topic refresh windows already processed |
| `passkeys` | WebAuthn credentials |

## Running Tests

```bash
just test
```

Integration tests spawn real servers on ephemeral ports and use isolated temp settings. `just ci` also runs `cargo fmt`, `clippy`, and a release frontend build.

## License

MIT. See [LICENSE](LICENSE).
