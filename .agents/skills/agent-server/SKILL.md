---
name: agent-server
description: >
  Run and verify Denpie for agent work: agent port :3027, never touch user port :3017,
  test login, UI DOM checks, server lifecycle. Use when starting the server, UI checks,
  browser verification, API smoke tests, or anything that binds/reuses Denpie ports.
  Triggers: agent server, :3027, :3017, UI check, test login, cargo run,
  localhost, --frontend-dist, playwright-astro.
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
just restart-agent-server    # stop ours + start fresh, return with it running
just stop-agent-server       # stop the server started by the recipe
just serve-local             # serve your real data (public schema) on :3027
just ui-check                # Astro build + oneshot agent-server
just frontend-astro-runtime  # alias of ui-check
just playwright              # Astro Playwright on :3027
just agent-server --frontend-dist frontend-astro/dist  # serve a pre-built Astro dist
```

`just serve-local` runs the same :3027 server against the **real** `public`
schema and the real `settings.yaml` / `tipcard-images` directory, so a human can
log in with their own account instead of the bootstrapped `test` login. It never
creates or resets an account and it never touches :3017. Use `just
restart-agent-server` afterwards to go back to the isolated agent schema.

`just restart-agent-server` is the recipe to run after finalizing a change the
running server serves (Rust, proto, migrations, frontend). It wraps
`--restart --detach`: it stops only the server this recipe started, starts a
fresh one, smoke-checks it, and returns with the server still running so an
agent can keep working. A listener it did not start is never killed. Use
`just agent-server` (foreground) or `--oneshot` when you want the script to own
the lifecycle instead.

`--detach` runs the server as a transient systemd **user** unit
(`denpie-agent-3027`) so it survives the caller exiting. A background job or
`setsid` stays inside the caller's cgroup, which an agent harness reaps at turn
end; the unit lives outside it. `just stop-agent-server` stops the unit. If
systemd is unavailable the script falls back to `setsid`.

The recipe:

1. Uses **only** `127.0.0.1:3027` / `http://localhost:3027`
2. Creates isolated data under `.agent-data/` and uses the `denpie_agent` PostgreSQL schema
3. Reuses a listener already on :3027 (never kills a pre-existing process)
4. Bootstraps test login when needed
5. Smoke-checks `GET /`, `GET /auth/me`, `GET /app/summary`
6. Cleans up the process **it** started (and data unless `--keep-data`)

Manual equivalent (only if you cannot use the recipe):

```bash
DENPIE_BIND_ADDR=127.0.0.1:3027 \
DENPIE_RP_ORIGIN=http://localhost:3027 \
DENPIE_DATA_DIR=.agent-data \
DATABASE_URL=postgres://denpie:denpie@127.0.0.1:5432/denpie \
DENPIE_DB_SCHEMA=denpie_agent \
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
- Visible UI: `just frontend-astro-test`, then DOM proof on `:3027`
- Stop if the user already sees the change or waives the check
- No over-verification unless asked

### Stateful fixtures

- With `DENPIE_DATA_DIR=.agent-data`, stored tip-card images live in
  **`.agent-data/tipcard-images/`**, not `.agent-data/images/`.
- Seed only the minimum row/file needed for the behavior under test, using the
  isolated `denpie_agent` schema. Remove only fixtures created by the check.
- Before debugging the browser, verify the exact API/resource boundary. For an
  image, request `/api/v1/tipcard-images/<id>` and confirm a successful response
  with the expected image content type.
- For a visible data-flow bug, prove the sequence once: persisted fixture → API
  response → DOM element (and screenshot when placement matters).
- A SPA navigation timeout can be a false negative. Inspect the current URL and
  DOM before retrying navigation or changing selectors.

`just ui-check` builds `frontend-astro/dist` and then stops its oneshot server.
For a follow-up DOM check, start `just agent-server --keep-data` and reuse
that build rather than invoking another frontend build.

The default dist is `frontend-astro/dist` and is auto-built when `index.html`
is missing. Custom `--frontend-dist` is never auto-built.

## API surface (agents)

- Stable client API: `POST /api` (protobuf). See `docs/agent-server-guide.md` and `docs/protobuf-api.md`.
- Browser dashboard: session auth on `/`, `/auth/*`, `/admin/*`, `/app/*`

## Startup shape

DB: PostgreSQL through `DATABASE_URL`; embedded migrations from `migrations/` run in the isolated `denpie_agent` schema.
