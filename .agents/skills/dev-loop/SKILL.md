---
name: dev-loop
description: >
  Denpie build, check, test, and CI commands for agents and humans. Use when
  verifying changes, running tests, starting watchers, or deciding how much to
  validate. Triggers: just check, just quick, just test, just test-one, just
  verify, just ci, cargo check, verify, build, lint, fmt, dev server,
  frontend-astro-test, playwright-astro.
---

# Dev loop (Denpie)

## Environment

```bash
just shell   # or nix-shell. Pins Rust, bun, protoc, PostgreSQL client
just         # list tasks
```

Prefer `just` recipes over raw cargo when they exist.

`just lab` is the opt-in research runner. It must never be added to
`just test`, `just verify`, or `just ci`; live lab image runs use the network.
Use `just lab-check` for the deterministic offline lab contract and
`just lab-cards-ui` for the Astro `FlowCard` fixture page on `:3027`.

## Tight edit loop

1. Check `git status --short` and preserve unrelated worktree changes.
2. Read the exact current hunk before patching. If a patch misses, re-read only
   that hunk and retry with a smaller patch instead of replaying a stale
   multi-file patch.
3. Prove the smallest relevant contract first: focused unit/integration test for
   data behavior, or one runtime DOM check for visible behavior.
4. Re-run only the failed focused command while iterating. Run one final gate
   after the focused checks pass.

Keep test helpers crate-local (`pub(crate)`) unless they are intentionally part
of the public library API. `cargo check` does not exercise every `#[cfg(test)]`
path, so compile at least one focused test before the final gate when changing
test-only exports.

## Verification tiers

Use the **smallest** gate that covers the edit. Do not re-run the full suite after every incremental change.

| Recipe | What it runs | When |
|---|---|---|
| `just quick` | `cargo fmt --check` + `cargo check --workspace` | Default while editing |
| `just check` | Alias of `quick` | Same |
| `just test-one <filter>` | `cargo test --workspace <filter>` | Logic change under one test module/name |
| `just test` | Full workspace tests (no frontend rebuild) | After a cluster of logic edits |
| `just verify` | fmt check + clippy + full tests | **Once** at end of a task / before commit |
| `just frontend-astro-test` | Astro i18n + topic-icon + Bun tests | Astro lib or UI logic |
| `just ui-check` | Astro build + isolated `:3027` oneshot smoke | Visible UI |
| `just lab-check` | Offline lab CLI, fixture, comparison, and lab-UI checks | Lab runner changes |
| `just ci` | `verify` + Astro tests and release build | PR / "fully done" |

```bash
just quick
just test-one grounding
just test-one test_dashboard_summary
just verify
just frontend-astro-test
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
| Visible UI change | `just frontend-astro-test`, then DOM proof on **:3027** via `just ui-check` / `just playwright` |
| About to commit / PR / "done" | **Exactly one** `just verify` (or `just ci` if frontend release matters) |
| User already confirmed UI | Stop; do not over-verify |

Skip a full frontend rebuild for routine agent loops unless the change is
frontend-only or you are running `just ci` / `just ui-check`. If `just ui-check`
already produced `frontend-astro/dist`, reuse it for the runtime DOM proof
instead of rebuilding it again.

## Stack reminder

- Rust 2024, Axum, PostgreSQL/SQLx, Tokio
- LLM via OpenAI-compatible endpoints
- Transport: protobuf `POST /api`
- Frontend: Astro + React islands in `frontend-astro/`

## Related skills

- **add-feature.** Where new code goes.
- **agent-server.** Ports and UI verification.
