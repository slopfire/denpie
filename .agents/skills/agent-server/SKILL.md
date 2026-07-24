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

## Preferred: automated recipe

```bash
just agent-server            # isolated data dir, bind :3027, test login, smoke, foreground
just agent-server --oneshot  # same but exit after smoke (used by just ui-check)
just agent-server --stop     # stop a server started by the recipe
just agent-server --smoke    # smoke only against an already-running :3027
just ui-check                # trunk release build + oneshot agent-server
```

The recipe:

1. Uses **only** `127.0.0.1:3027` / `http://localhost:3027`
2. Creates isolated data under `.agent-data/` (settings, DB, images)
3. Reuses a listener already on :3027 (never kills a pre-existing process)
4. Bootstraps test login when needed
5. Smoke-checks `GET /`, `GET /auth/me`, `GET /app/summary`
6. Cleans up the process **it** started (and data unless `--keep-data`)

Manual equivalent (only if you cannot use the recipe):

```bash
DENPIE_BIND_ADDR=127.0.0.1:3027 \
DENPIE_RP_ORIGIN=http://localhost:3027 \
DENPIE_DATA_DIR=.agent-data \
DENPIE_SKIP_FRONTEND_BUILD=1 \
cargo run
```

## Test login

- Username: `test`
- Password: `23452345`

## UI checks

- Target: **`http://localhost:3027` only**
- Assert **concrete DOM IDs and placement**
- Skip brittle a11y names and `waitForUrl` for SPA routes
- Simple visible UI change → `just quick` + targeted DOM check
- Stop if the user already sees the change or waives the check
- No over-verification unless asked

## API surface (agents)

- Stable client API: `POST /api` (protobuf) — see `docs/agent-server-guide.md` and `docs/protobuf-api.md`
- Browser dashboard: session auth on `/`, `/auth/*`, `/admin/*`, `/app/*`

## Startup shape

DB: `schema.sql`, then compatibility migrations in `src/db/migrations.rs`.
