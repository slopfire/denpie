Denpie is a tool specialized on creating a flow of tips based on topic.
The problem it should decide is to deliver intel on user choosen problem information dense, but most importantly
it should make the user to actually learn the thing.
The main goal of the app is to make User better at something.
Second, important goal for this app is to be flexible, adaptable to be used by different tools and situations.

## Skills (multi-agent)

Procedures live in **`.agents/skills/`** so Grok, Claude, Cursor, and other agents share them. Load the matching skill when the task fits:

| Skill | When |
|---|---|
| [`dev-loop`](.agents/skills/dev-loop/SKILL.md) | `just check` / test / ci / how much to verify |
| [`agent-server`](.agents/skills/agent-server/SKILL.md) | Run server, ports, UI checks, test login |
| [`add-feature`](.agents/skills/add-feature/SKILL.md) | New feature, layer placement, integration checklists |

Invariants below stay here. Do not duplicate long procedures in this file.

<!-- CODEGRAPH_START -->
## CodeGraph

When `.codegraph/` exists, use CodeGraph for targeted cross-file tracing after
you know at least one relevant file or exact symbol. Keep the query narrow and
limit returned source, for example:

```bash
codegraph explore --max-files 4 "<file> <exact symbol> callers"
```

Use `rg` to locate exact text, routes, tests, and references. Treat CodeGraph
callers, blast radius, dynamic dispatch, and test coverage as leads, then verify
them against current source. If one refined query still returns unrelated or
truncated results, continue with `rg` and direct reads.

Check availability with `codegraph status`. A missing index ends the CodeGraph
branch; indexing remains the user's decision.
<!-- CODEGRAPH_END -->

## Invariants

1. **Ports:** never touch **`:3017`** (user). Agents use **`:3027`** only. See `agent-server`.
2. **Layers:** domain (pure) → services → repositories → thin `api` / `dashboard`. See `add-feature`.
3. **Scheduling:** SM-2 only. `FSRS` is a legacy alias. Do not claim real FSRS.
4. **SQL:** bound parameters only in repositories.
5. **Docs:** change code → update docs/examples in the same change.
6. **API v1:** additive-only wire contract; every operation must declare its result,
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
just ui-check       # Astro build + agent oneshot smoke
just frontend-astro-test  # Astro catalog + Bun tests
just ci             # full gate including Astro release build
just lab list       # opt-in research runner; see docs/lab.md
just lab-check      # deterministic offline lab contract
just lab-cards-ui   # Astro FlowCard fixture page on :3027
just lab-cards-dev  # Astro production-card fixture page with HMR on :3027
just lab-review <baseline> <candidate>  # blinded A/B artifact review on :3027
```

Detail: `dev-loop` skill. Feature paths: `docs/feature-integration.md`. Astro UI: `docs/frontend-astro.md`. API for agents: `docs/agent-server-guide.md`.

## Stack

- Rust 2024, Axum, PostgreSQL via SQLx, Tokio
- LLM: `async-openai` against OpenAI-compatible endpoints
- Transport: protobuf (`prost`), `POST /api`
- Frontend: Astro + React islands in `frontend-astro/`
