---
name: dev-loop
description: >
  Denpie build, check, test, and CI commands for agents and humans. Use when
  verifying changes, running tests, starting watchers, or deciding how much to
  validate. Triggers: just check, just test, just ci, cargo check, verify,
  build, lint, fmt, dev server.
---

# Dev loop (Denpie)

## Environment

```bash
just shell   # or nix-shell — pins Rust, Trunk, protoc, SQLite, OpenSSL
just         # list tasks
```

Prefer `just` recipes over raw cargo when they exist.

## Commands

| Recipe | Use |
|---|---|
| `just check` | Fast: `cargo check --workspace` (skips frontend rebuild) |
| `just test` | `cargo test --workspace` (skips frontend rebuild) |
| `just fmt` | `cargo fmt --all` |
| `just lint` | clippy with `-D warnings` |
| `just ci` | fmt check + clippy + tests + release frontend build |
| `just dev` | backend + frontend watchers |
| `just backend` | backend only (`DENPIE_SKIP_FRONTEND_BUILD=1 cargo run`) |
| `just frontend` | Trunk watch |

## How much verification

| Situation | Do this |
|---|---|
| Small / local code change | `just check` |
| Logic / repo / API change | `just check` then targeted `cargo test` or `just test` |
| Visible UI change | `just check` + DOM check on **:3027** (see `agent-server` skill) |
| About to commit / PR / “done” | `just ci` or at least check + tests that cover the change |
| User already confirmed UI | Stop; do not over-verify |

Skip full frontend rebuild for routine agent loops unless the change is frontend-only or you are running `just ci`.

## Stack reminder

- Rust 2024, Axum, SQLite/SQLx, Tokio
- LLM via OpenAI-compatible endpoints
- Transport: protobuf `POST /api`
- Frontend: Yew/WASM + Tailwind

## Related skills

- **codegraph** — find code before editing
- **add-feature** — where new code goes
- **agent-server** — ports and UI verification
