# Astro frontend

Put UI in `frontend-astro/`. Astro emits a static `dist/`. Axum is the only server.
`DENPIE_FRONTEND_DIST` defaults to `frontend-astro/dist`.

## Homes

| Need | Path |
|---|---|
| Static route | `frontend-astro/src/pages/<name>.astro` |
| Session shell | `frontend-astro/src/islands/AppShell.tsx` |
| Page / Flow UI | `frontend-astro/src/components/pages/`, `.../flow/` |
| Registry primitives | `frontend-astro/src/components/ui/` via the shadcn CLI |
| Transitions, mapping, parsers | `frontend-astro/src/lib/` |
| Flow `/api/v1` helpers | `frontend-astro/src/lib/api-v1/ops.ts` |
| Other-route `/api/v1` helpers | `frontend-astro/src/lib/api-v1/route-ops.ts` |
| Wire types | `frontend-astro/src/generated/denpie_pb.ts` |
| English catalog | `frontend-astro/src/i18n/en.json` |
| Theme tokens | `frontend-astro/src/styles/global.css` |
| Browser proof | `tests/e2e-astro/` |

## Run it

`just` lists the recipes. The traps:

- `just dev` is the live loop. Open `http://localhost:4321/`. Vite proxies `/api`, `/auth`, `/app`, `/admin`, and `/static` to Axum. `:3017` serves a built `frontend-astro/dist` when one exists.
- Keep `vite` as a direct `frontend-astro` dependency (Astro 7's Vite 8). `@vitejs/plugin-react` detects Rolldown/Oxc by importing `vite`; a hoisted Vite 6 peer makes it emit `esbuild` / `optimizeDeps.esbuildOptions` deprecation warnings. The `overrides.vite` pin keeps one copy in the tree.
- `just agent-server` serves `frontend-astro/dist` on `:3027` and auto-builds it when `index.html` is missing. A custom `--frontend-dist` is never auto-built.
- `just ui-check` / `just frontend-astro-runtime` builds, then smoke-checks on `:3027`.
- `/lab-cards` is the opt-in fixture gallery (see `docs/lab.md`). It is
  standalone by design — it does not use `AppLayout`, renders no auth gate,
  and is excluded from `ui-check`/`ci`. Its fixture data is the checked-in
  `lab/cases/cards/repeatable-states.json`, inlined at build time via the
  `__LAB_CARD_FIXTURES__` vite `define` in `astro.config.mjs`.
- The app registers `/service-worker.js` (Axum serves
  `frontend-astro/public/service-worker.js`). The worker caches hashed
  `/_astro/` assets cache-first and deletes the whole asset cache once it
  exceeds 250 entries, so deploys cannot grow it without bound.
- Read ops in `src/lib/api-v1/ops.ts` go through a 30-second TTL read cache
  (`cache.ts`); every successful mutation invalidates it. The first real
  `/auth/me` call also fires `bootstrap-prefetch`, racing the first
  `list_flow_cards` page against the session check.

## Conventions

**Catalog.** Visible copy, labels, placeholders, feedback, confirmations, alt text, and accessibility names go through `t` / `tf` from `@/lib/i18n`. Keys are a TypeScript union of `frontend-astro/src/i18n/en.json`. Protocol IDs stay raw until the UI edge maps the known ones. Unknown values stay visible. `scripts/check-astro-i18n.mjs` fails the test recipe when TSX bypasses the catalog.

**Registry.** Add primitives with `bunx --bun shadcn@latest add <name>` from `frontend-astro/`. Keep `shadcn diff` clean. App screens compose those files. They do not reimplement them.

**Tokens.** Semantic Tailwind only (`bg-primary`, `text-foreground`, `bg-card`). New variables go in every `[data-theme]` block in `src/styles/global.css`. Saved theme IDs are `shadcn`, `shadcn-light`, `carbonfox`, `ayu`, `solarized-light`, `solarized-dark`, `amoled`, and `slate`. Native overflow follows the `ScrollArea` thumb: 10px, rounded, `--border`. Compose `ScrollArea` for bounded panes; keep `shadcn diff` clean.

**Pure modules.** `src/lib/*.ts` owns discriminated unions and transitions. Components call those functions. Fetch lives in `src/lib/api-v1/` with injectable `fetch`. Requests start from effects or event handlers, never from a setState updater. Stale completions die on a generation counter. Card IDs stay `bigint`.

**Mutations.** The caller allocates the idempotency key. An outcome-indeterminate failure retries with the same key and payload. A determinate failure may allocate a new key. `TransportError.mutationOutcomeIndeterminate` is the transport's own verdict. Post-dispatch errors that cannot prove the mutation missed the server stay indeterminate.

**Layout.** Authenticated desktop uses a fixed 14rem left rail and `lg:ml-56` main. Mobile uses a five-item labeled dock in the same column as main (main scrolls, the dock stays), in the same order as the rail. Guest sign-in is centered in the main column. The main area owns scroll. `/` is Flow, `/flow` is an alias, then `/grounding`, `/settings`, `/keys`, `/archive`, `/account`. In-app navigation uses the History API so the shell stays mounted; swaps animate through the View Transitions API via `src/lib/view-transition.ts` (fade/slide on `main.app-main`, skipped under reduced motion or unsupported browsers). Flow stays mounted after the first visit so review state survives navigation. Archive and the other routes unmount when you leave (`src/lib/keep-alive.ts`), so inventory markdown and decoded images are not kept in memory. Archive list cards hydrate markdown and images only near the viewport; list thumbnails decode at 640px (`src/lib/image-thumbnail.ts`) instead of the stored 2048px files. The active view refreshes on focus and every 60 seconds; with rendered data that refresh is silent — page 1 refetches in the background and merges through `src/lib/flow-refresh-merge.ts` (skipped while a pin/delete/add mutation is in flight), so no skeleton flash or scroll reset. Only a true cold start shows the loading screen, optionally seeded from the session snapshot (`denpie-flow-snapshot`, 10-minute max age) with a dimmed `revalidating` grid.

**Flow review.** Repeatable tips keep Again/Learned as primary buttons and put Known / Not interested / Too difficult in one skip-reasons menu. The menu opens on hover (click still works); the chevron points up when closed and rotates 180° while the menu is open. Topic picks hug their content (no fixed card min-height), left-align the topic name with the other app headings, and say how many cards are showing rather than `n/9`. The title bar (topic icon, name, New/Known, expand) is one `items-center` row with equal `--card-spacing` padding above the separator; the expand control is `icon-xs` so it matches the status chip. Again or Learned replaces a pick with the next card. The add form and sort/layout toolbar sit on the right from `sm`; the form is full width on mobile. Grid gap is `gap-6` to match main padding.

**Flow fullscreen.** Repeatable-card detail stays open across review replacement and Continue. The overlay follows the topic slot (`flowSlotKey`), not the consumed card ID. On desktop the overlay and card stay to the right of the 14rem rail, so the sidebar stays visible and clickable. The overlay portals into `#flow-view`, so keep-alive hides it with Flow. Escape, the close button, the fullscreen trigger, and a click on the empty frame still dismiss it. Review, Continue, pin, and other in-overlay controls do not.

**Continue progress.** A `continuing` slot shows a live elapsed counter
(`flow.continuing_elapsed_description`) after `CONTINUE_TICK_AFTER_SECONDS`
(5s) and switches to an explicit patience message
(`flow.continuing_slow_description`) at `CONTINUE_SLOW_AFTER_SECONDS` (45s);
both live in `continuingStatusText` in `src/lib/flow-review-state.ts`. The
slot records its `startedAt` so retries restart the clock, and the slow path
covers research-grade backend generations that legitimately take minutes.

**Topic icons.** `scripts/generate-topic-icons.mjs` checks in the allowlisted subset from `config/topic_icons.json`. Render through `@iconify/react/offline`. Cards do not fetch a CDN and do not bundle whole collections. Grounding topic cards use a compact grid (icon + name + type badge, due/total, pending/scheduled, Load/Edit; Delete lives in the `…` menu). The icon button opens the AI picker: session JSON `POST /app/topics/suggest-icons` and `POST /app/topics/set-icon`. Helpers live in `src/lib/topic-icon-picker.ts`. Missing pool or card images collapse or show a placeholder instead of the browser broken-image glyph.

**Prompt templates.** Settings and the topic editor expose Reset and Enhance next to the template. Reset pastes the current prompt into the field (the built-in default on Settings, the global settings template on a topic). Enhance calls `enhance_prompt_template` (`topic_id` 0 for Settings) and copies the suggestion into the draft, including grounding fields when they are non-empty. Save still persists. Helpers live in `src/lib/pages/prompt-template.ts`.

## Add a page

Done when the static route, `AppView` discriminant, History mapping, catalog title, and a focused test all exist, and `just frontend-astro-test` passes.

1. Add `frontend-astro/src/pages/<name>.astro` that mounts `AppLayout` with a catalog title and the new `view`.
2. Extend `AppView`, `viewForPathname`, `pathnameForView`, and `titleKeyForView` in `AppShell.tsx`.
3. Render the page from `AuthenticatedView`.
4. Add a rail or dock item only for a working route.
5. Put transitions in `src/lib/` and the React tree in `src/components/pages/`.
6. Cover the new mapping with a Bun test next to the module.

## Add a string

Done when the key exists in `frontend-astro/src/i18n/en.json`, every visible site uses `t`/`tf`, and `just frontend-astro-i18n-check` is clean.

1. Add the key under the surface group (`nav.*`, `auth.*`, `toast.*`, `confirm.*`, …)
2. Call `t("group.key")` or `tf("group.key", { name: value })`. `tf` arguments must match the `{placeholders}` in the message.
3. Map protocol and storage IDs at the UI edge. Keep the raw ID visible when the map has no entry.

## Add a registry primitive

Done when `src/components/ui/<name>.tsx` is CLI output and `shadcn diff <name>` is clean.

1. From `frontend-astro/`: `bunx --bun shadcn@latest add <name>`
2. Compose it from app components. Semantic classes only.
3. If a token is missing, add `--<name>` (and `--<name>-foreground` when needed) to every `[data-theme]` block.

## Wire an `/api/v1` operation

Done when the helper uses generated request/result types, rejects the wrong result case, preserves `bigint` IDs, and a Bun test covers the request shape plus the success and main failure mappings.

1. If `proto/denpie.proto` changed, run `just frontend-astro-protogen`. `just frontend-astro-proto-check` must pass. The generated file is output, not a hand-edit.
2. Add the helper to `ops.ts` (Flow) or `route-ops.ts` (other routes). Reads go through `callEnvelope`. Mutations go through `callMutationWithKeyEnvelope` with a caller-owned key.
3. Require the expected `result.case`. Map an empty wire `next_page_token` to absence.
4. Keep page state in a pure module so illegal combinations are unrepresentable.

## Prove it

| Change | Gate |
|---|---|
| Pure module / mapper | `just frontend-astro-test` |
| Visible copy | that recipe already runs the i18n check |
| Proto types | `just frontend-astro-proto-check` |
| Served UI | `just ui-check`, then `just playwright` on `:3027` |
