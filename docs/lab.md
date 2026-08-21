# Denpie Lab

`denpie-lab` is the **opt-in research runner** for Denpie. It shares production
library code (`src/lab/` lives inside the same crate as the server) so a
researcher can exercise the real image-retrieval path, inspect repeatable
card UI states in the production Yew component, record mechanical results,
compare runs, and leave visual judgement to a human.

The lab is **not CI**. Live image and prompt benches make network requests and
are inherently slow/flaky, so `just test`, `just verify`, and `just ci` never
start a live lab run. The cards gallery is local-only. The CI-safe lab
commands are `list`, `run images --dry-run`, `run prompts --dry-run`, and
`run cards --dry-run` (the full `run cards` gallery is also local-only, so
it never touches the network).

## Commands

```bash
just lab list                          # print benches, status, and one-line purpose
just lab run images --dry-run          # load gold cases, print the plan, no downloads
just lab run images                    # LIVE run: network allowed
just lab run images --offline          # alias for --dry-run
just lab run images --strategy all --dry-run
just lab run images --strategy bing_html,bing_playwright --dry-run
just lab run images --cases lab/cases/images/gold.json --dry-run
just lab run images --concurrency 1
just lab run prompts --dry-run          # assemble prompts, no LLM calls
just lab run prompts --offline          # alias for --dry-run
just lab run prompts --cases lab/cases/prompts/gold.json --dry-run
just lab run prompts                    # LIVE: one-shot cases call generate_card
just lab run cards --dry-run            # print repeatable-card fixture states
just lab run cards --offline            # alias for --dry-run
just lab run cards --cases lab/cases/cards/repeatable-states.json --dry-run
just lab run cards                      # write a static catalog + JSON (no network)
just lab compare <baseline.json> <candidate.json>
just lab-check                          # deterministic offline lab contract
just lab-cards-ui                       # real FlowCard fixtures on :3027
```

- Default image strategy is `bing_html` only.
- `--strategy` is repeatable or comma-separated; `all` expands to
  `bing_html,bing_playwright,ddgs_text_og`.
- Live jobs overlap. Default `--concurrency` is 5, capped at the job count.
  Playwright discovery stays at one browser at a time. Use `--concurrency 1`
  to force a sequential run.
- Each live job has a 90s deadline. A timeout is a miss (`kind: timeout`),
  not a non-zero exit.
- `libcaesium` alone uses `opt-level = 3` in the shared dev profile. This keeps
  PNG recompression usable without duplicating the whole dependency graph in
  a separate Cargo profile.
- Unknown flags, unknown benches, and `run algorithms` exit `2`.
- Live `run prompts` requires non-empty `DENPIE_LAB_LLM_API_KEY`. Optional:
  `DENPIE_LAB_LLM_MODEL` (default `google/gemini-3.1-flash`) and
  `DENPIE_LAB_LLM_BASE_URL` (default `https://openrouter.ai/api/v1`).
- Array-mode prompt cases are assembled and recorded as
  `kind: assembled_only`; live array generation is not wired yet.
- Case IDs must match `[a-z0-9]+(-[a-z0-9]+)*`; duplicates and path-like IDs
  are rejected before any artifacts are created.

## Benches

| Bench | Status | What it does |
|---|---|---|
| `images` | ready | Runs the five-card image bake-off gold set through production `retrieve_image` and writes a scorecard under `lab/runs/`. |
| `algorithms` | planned | Scheduler bake-off. Not implemented; SM-2 remains the only production scheduler. |
| `prompts` | ready | Assembles one-shot and array prompts from the prompt gold pack; live one-shot cases call production `generate_card`. |
| `cards` | ready | Writes a static fixture catalog plus JSON. `just lab-cards-ui` renders the same data with production `FlowCard`. |

## Cards gold set

`lab/cases/cards/repeatable-states.json` is the repeatable-card fixture pack.
It is a JSON array with exactly seven fixtures: `active`, `pinned`,
`reviewed-hold`, `await-refill`, `daily-complete`, `stacked`, and
`llm-error`. Each fixture stores `id`, `topic_name`, `title`, `full_content`,
`compressed_content`, `tipcard_type` (`repeatable_tip`), `status` (`active`
or `reviewed`), `pinned`, `pending_count`, optional `review_message`, and
`notes`.

`run cards --dry-run` loads the pack and prints each fixture id, topic,
status, pinned flag, pending count, and whether `review_message` is set.
It writes nothing under `lab/runs/`.

## Production card UI lab

`just lab-cards-ui` builds the frontend with the opt-in `lab-ui` feature and
serves it on the agent-only port `:3027`. The feature replaces the normal app
root with the checked-in fixtures wrapped in the real production `FlowCard`.
It supports expand, fullscreen, pin, delete, and local review/Continue
interactions. Image attach/clear controls are disabled, so the lab cannot
write server image state.

Review and Continue are deliberately local simulations. They prove the
component states and event wiring, not `UnifiedFlow` replacement/refill or API
persistence; those remain production integration concerns and test targets.
Normal frontend builds do not include the card-lab module.

## Static card catalog output

`run cards` is local-only: it loads the same fixture pack and writes:

```text
lab/runs/<utc-timestamp>-cards/
├── gallery.html              # self-contained HTML fixture gallery
└── gallery.json              # same fixtures as JSON
```

`gallery.html` is a lightweight catalog, not production rendering. It has
inline CSS only, no network assets, and no JS framework.
It contains one section per fixture with topic and title, badges for status,
pinned, pending count, and review message, a stack-layer count
(`pending_count.min(3)` for `repeatable_tip`), and the compact and full text.
All fixture text is HTML-escaped before it is embedded.

The runs directory is gitignored (`lab/runs/`).

## Image gold set

`lab/cases/images/gold.json` is the five-card gold set from the Bing HTML
bake-off in [`docs/image-fetch-bing-html.md`](image-fetch-bing-html.md)
(the sibling probes are
[`docs/image-fetch-playwright-bing.md`](image-fetch-playwright-bing.md) and
[`docs/image-fetch-ddgs-text-og.md`](image-fetch-ddgs-text-og.md)). Each case
stores `id`, `topic_name`, `card_title`, a short `card_content`, the exact
`image_query`, and a human `expected` visual rubric.

The parser and policy tests use the checked-in fixtures under
`src/llm/images/fixtures/`. Live Bing / Playwright / DDG runs never belong in
`just test` / CI.

## Live run output

A live `run images` calls production `crate::llm::retrieve_image` once per case
and strategy, with empty model/API-key/base settings, `ReasoningConfig::new("none")`,
and an empty pool. Jobs run concurrently and print a progress line as each
finishes. The scorecard is written in case-pack order. It writes:

```text
lab/runs/<utc-timestamp>-images/
├── scorecard.md              # mechanical table + human visual rubric
├── scorecard.json            # same data as JSON
└── cases/<id>/<strategy>.<ext>
```

Mechanical columns (`search_or_download`, `kind`, `bytes`, `mime_type`,
`extension`, `elapsed_ms`) are recorded automatically. `kind` is `prepared`,
`pool:<id>`, `none`, or `timeout`. The `visual` column is always
`needs_review`: bytes are never used to invent a pass/fail. A human compares
each downloaded image with the `expected` rubric and updates the scorecard. A
miss is a result, so a live run exits `0` even when every strategy misses.

The runner creates empty scorecards as soon as a live run starts, then
atomically checkpoints both formats after every completed job in case-pack
order. Run directories use sub-millisecond timestamps plus a collision suffix,
so two processes do not silently share an output directory. The 90-second
deadline covers queueing, retrieval, and artifact writing.

The runs directory is gitignored (`lab/runs/`).

## Prompt gold set

`lab/cases/prompts/gold.json` is the prompt bake-off case pack. Each case
stores `id` (string), `topic`, optional `template` (empty/omitted uses
`DEFAULT_PROMPT_TEMPLATE`), `compression` (default `strong`), `mode`
(`one_shot` or `array`), `batch_count` (default 5, used for array mode),
five context title arrays (`existing_titles`, `dismissed_titles`,
`known_items`, `difficult_items`, `uninterested_items`; all default empty),
and a human `expected` rubric.

The checked-in pack covers English Grammar with existing/known titles, Rust
with empty context, Helix Editor with empty context, an array case with
`batch_count: 5`, and a custom template containing `{topic}` and
`{existing_cards}`.

## Prompt live run output

A live `run prompts` calls production `crate::llm::generate_card` for each
`one_shot` case with `ReasoningConfig::new("none")` and the configured
OpenRouter-compatible model. Array cases are assembled but not generated. It
writes:

```text
lab/runs/<utc-timestamp>-prompts/
├── scorecard.md              # mechanical table + human visual rubric
├── scorecard.json            # same data as JSON
└── cases/
    ├── <id>.prompt.txt       # full assembled prompt
    └── <id>.card.json        # generated card, one-shot hit only
```

Mechanical columns (`assembled`, `generated`, `kind`, `title_words`,
`full_content_words`, `compressed_content_words`, `use_image`,
`prompt_tokens`, `completion_tokens`, `total_tokens`, `elapsed_ms`, and a
redacted `error`) are recorded automatically.
`generated` is `hit` or `miss`; a transport/parse failure is a miss with
`kind: error`, not a non-zero exit. The `visual` column is always
`needs_review`; a human compares each generated card with the `expected`
rubric. Missing `DENPIE_LAB_LLM_API_KEY` exits `2`.

Prompt scorecards are atomically checkpointed after every case, and each live
generation has a 90-second timeout. Stored failures keep the useful error
category while removing credentials and URL query values.

The runs directory is gitignored (`lab/runs/`).

## Comparing runs

`just lab compare <baseline-scorecard.json> <candidate-scorecard.json>` reads
two image scorecards or two prompt scorecards without network access. Rows are
matched by `case_id/strategy` for images and `case_id` for prompts. The report
lists added and removed rows, mechanical outcome changes, and signed deltas for
latency, bytes, word counts, and token counts. It rejects mixed bench types,
ambiguous empty scorecards, duplicate row identities, and malformed fields.

A valid comparison exits `0` even when outcomes regress; the command reports
evidence and leaves the decision to the researcher.

## Offline contract check

`just lab-check` is the rerunnable proof for lab development. It exercises
offline plans, strategy deduplication, unsafe-ID rejection, scorecard
comparison, the focused Rust lab tests, the checked-in card fixture mapping,
and a `lab-ui` frontend check. It never runs live network benches and never
writes under `lab/runs/`.
