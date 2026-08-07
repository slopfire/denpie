---
name: dev-loop
description: >
  Denpie build, check, test, and CI commands for agents and humans. Use when
  verifying changes, running tests, starting watchers, or deciding how much to
  validate. Triggers: just check, just quick, just test, just test-one, just
  verify, just ci, cargo check, verify, build, lint, fmt, dev server.
---

# Dev loop (Denpie)

## Environment

```bash
just shell   # or nix-shell — pins Rust, Trunk, protoc, SQLite, OpenSSL
just         # list tasks
```

Prefer `just` recipes over raw cargo when they exist.

## Verification tiers

Use the **smallest** gate that covers the edit. Do not re-run the full suite after every incremental change.

| Recipe | What it runs | When |
|---|---|---|
| `just quick` | `cargo fmt --check` + `cargo check --workspace` | Default while editing |
| `just check` | Alias of `quick` | Same |
| `just test-one <filter>` | `cargo test --workspace <filter>` | Logic change under one test module/name |
| `just test` | Full workspace tests (no frontend rebuild) | After a cluster of logic edits |
| `just verify` | fmt check + clippy + full tests | **Once** at end of a task / before commit |
| `just ui-check` | Release frontend build + isolated `:3027` oneshot smoke | Visible UI change |
| `just ci` | `verify` + release frontend build | PR / “fully done” |

```bash
just quick
just test-one grounding
just test-one test_dashboard_summary
just verify
just ui-check
```

## Agent server

Isolated runtime on **:3027 only** (never :3017):

```bash
just agent-server            # start, bootstrap test user, smoke, foreground
just agent-server --oneshot  # start, smoke, stop
just agent-server --stop     # stop a server we started
just agent-server --smoke    # smoke against already-running :3027
```

Detail: `agent-server` skill.

## How much verification

| Situation | Do this |
|---|---|
| Small / local code change | `just quick` |
| Logic / repo / API change | `just quick` then `just test-one <filter>` |
| Visible UI change | `just quick` + DOM check on **:3027** (see `agent-server`) or `just ui-check` |
| About to commit / PR / “done” | **Exactly one** `just verify` (or `just ci` if frontend release matters) |
| User already confirmed UI | Stop; do not over-verify |

Skip full frontend rebuild for routine agent loops unless the change is frontend-only or you are running `just ci` / `just ui-check`.

## Stack reminder

- Rust 2024, Axum, PostgreSQL/SQLx, Tokio
- LLM via OpenAI-compatible endpoints
- Transport: protobuf `POST /api`
- Frontend: Yew/WASM + Tailwind

## Related skills

- **codegraph** — find code before editing
- **add-feature** — where new code goes
- **agent-server** — ports and UI verification
