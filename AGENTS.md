# AI Agent Instructions (Denpie)

Daily tip cards. SM-2 scheduling. Do not claim real FSRS until the code has real FSRS.

## Skills (multi-agent)

Procedures live in **`.agents/skills/`** so Grok, Claude, Cursor, and other agents share them. Load the matching skill when the task fits:

| Skill | When |
|---|---|
| [`codegraph`](.agents/skills/codegraph/SKILL.md) | Understand / locate code — **before** grep/read loops |
| [`dev-loop`](.agents/skills/dev-loop/SKILL.md) | `just check` / test / ci / how much to verify |
| [`agent-server`](.agents/skills/agent-server/SKILL.md) | Run server, ports, UI checks, test login |
| [`add-feature`](.agents/skills/add-feature/SKILL.md) | New feature, layer placement, integration checklists |

Invariants below stay here. Do not duplicate long procedures in this file.

## Invariants

1. **Ports:** never touch **`:3017`** (user). Agents use **`:3027`** only. See `agent-server`.
2. **Layers:** domain (pure) → services → repositories → thin `api` / `dashboard`. See `add-feature`.
3. **Scheduling:** SM-2 only. `FSRS` is a legacy alias — do not claim real FSRS.
4. **SQL:** bound parameters only in repositories.
5. **Docs:** change code → update docs/examples in the same change.
6. **CodeGraph:** if `.codegraph/` exists, use it before search/read thrash. See `codegraph`.
7. **API v1:** additive-only wire contract; every operation must declare its result,
   scope, mutation/idempotency policy, docs, and tests. Run `just api-check`; see
   `docs/api-development-rules.md`.

## Quick start

```bash
just shell          # pinned toolchain
just db-up          # local PostgreSQL
just quick          # fmt check + compile (default while editing)
just api-check      # v1 wire/operation/result/docs contract
just test-one <f>   # targeted tests
just verify         # one full gate at end of a task
just agent-server   # isolated :3027 runtime + test login + smoke
just ui-check       # frontend release build + agent oneshot smoke
just ci             # full gate including frontend release build
```

Detail: `dev-loop` skill. Feature paths: `docs/feature-integration.md`. API for agents: `docs/agent-server-guide.md`.

## Stack

- Rust 2024, Axum, PostgreSQL via SQLx, Tokio
- LLM: `async-openai` against OpenAI-compatible endpoints
- Transport: protobuf (`prost`), `POST /api`
- Frontend: Yew/WebAssembly + Tailwind
