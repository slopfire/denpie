---
name: agent-server
description: >
  Run and verify Denpie for agent work: agent port :3027, never touch user port :3017,
  test login, UI DOM checks, server lifecycle. Use when starting the server, UI checks,
  browser verification, API smoke tests, or anything that binds/reuses Denpie ports.
  Triggers: agent server, :3027, :3017, UI check, test login, cargo run, localhost.
---

# Agent server (Denpie)

## Ports (non-negotiable)

| Port | Who | Rule |
|---|---|---|
| **`:3017`** | Human / user | **Never** bind, reuse, inspect, restart, or stop |
| **`:3027`** | Agents | Only port for agent UI and server work |

```bash
DENPIE_BIND_ADDR=127.0.0.1:3027 DENPIE_RP_ORIGIN=http://localhost:3027 cargo run
```

Any skill or habit that says “reuse :3017” is wrong for this repo. Use **:3027 only**.

## Server lifecycle

1. Check whether something already listens on **:3027**.
2. If Denpie is already up there → **reuse it**. Never kill a pre-existing process.
3. If not running → start with the env above (or `just backend` with the same bind overrides).
4. Stop **only** servers you started. Close your own `cargo run` when done.

## Test login

- Username: `test`
- Password: `23452345`

## UI checks

- Target: **`http://localhost:3027` only**
- Assert **concrete DOM IDs and placement**
- Skip brittle a11y names and `waitForUrl` for SPA routes
- Simple visible UI change → `just check` + targeted DOM check
- Stop if the user already sees the change or waives the check
- No over-verification unless asked

## API surface (agents)

- Stable client API: `POST /api` (protobuf) — see `docs/agent-server-guide.md` and `docs/protobuf-api.md`
- Browser dashboard: session auth on `/`, `/auth/*`, `/admin/*`, `/app/*`

## Startup shape

DB: `schema.sql`, then compatibility migrations in `src/db/migrations.rs`.
