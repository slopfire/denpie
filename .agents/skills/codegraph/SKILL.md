---
name: codegraph
description: >
  Explore and navigate the Denpie codebase with CodeGraph before grep/find/read loops.
  Use when understanding how code works, locating symbols, surveying an area, tracing
  call paths, or before editing code. Triggers: codegraph, how does X work, where is X,
  explore codebase, find callers, architecture of, before implementing.
---

# CodeGraph (Denpie)

This repo has a `.codegraph/` index. Prefer CodeGraph over ad-hoc search whenever you need to understand or locate code.

## Rule

**Before** grep/find or bulk file reads for “how does X work”, “where is X”, architecture, a bug hunt, or symbols you are about to change — call CodeGraph first.

If `.codegraph/` is missing at the repo root, skip this skill entirely. Indexing is the user’s choice.

## How to call (in priority order)

1. **MCP** (when available): `codegraph_explore` with a natural-language question or symbol/file names. Returns verbatim source grouped by file plus call paths. Treat returned source as already read — do not re-open those files.
2. **Shell** (always works):

```bash
codegraph explore "<symbol names or question>"
codegraph node <symbol-or-file>
```

Same content as the MCP tools.

## Query tips

- Prefer one capped explore call over many greps.
- For flows, name symbols that span the path (e.g. `force_daily_refresh tips service`).
- For edits, explore the symbols you will change so callers/blast radius are visible.
- Natural language works: `"how does document grounding attach to topics?"`

## After CodeGraph

- Edit from the returned line-numbered source.
- Use targeted reads only for files CodeGraph did not cover.
- Do not re-derive call graphs with grep when CodeGraph already returned them.
