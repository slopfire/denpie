# Task
Finish denpie-lab. Four pieces in this change: (1) fix ambiguous `cargo run` that broke `just dev`, (2) implement the `prompts` bench, (3) implement the `cards` bench, (4) implement the `algorithms` bench including a clock argument on SM-2. Do not add HTTP routes, protobuf ops, or bind :3017/:3027. Do not add a second production scheduler. `FSRS` stays a storage alias.

# Repository facts
- workspace: /home/sfire/Projects/slopfire/denpie
- `cargo run` currently fails with:

```
error: `cargo run` could not determine which binary to run. Use the `--bin` option to specify a binary, or the `default-run` manifest key.
available binaries: denpie, denpie-lab
```

Broken callers: `justfile` recipe `backend` (`DENPIE_SKIP_FRONTEND_BUILD=1 cargo run`), `scripts/dev.sh` line 14, `scripts/agent-server.sh` (~line 147), `benches/run_bench.sh` (~line 49). README documents `cargo run`.

- Lab CLI: `src/lab/mod.rs`. Images is ready. This match is still stubs:

```
"algorithms" | "prompts" | "cards" => {
    usage_error(stderr, &format!("bench `{bench}` is not implemented"))
}
```

`list_benches()` and the test `list_benches_reports_images_ready_and_three_planned` and `run_algorithms_exits_two_not_implemented` must be updated. After this task all four benches are `ready`. Delete `run_algorithms_exits_two_not_implemented` or change it to a dry-run success test.

- Image bench pattern to copy: `src/lab/images.rs`, `src/lab/cases.rs`, `lab/cases/images/gold.json`. `--dry-run`/`--offline` print a plan and write nothing under `lab/runs/`. Live writes `lab/runs/<utc>-<bench>/`. No clap. No new crates. Manual argv parse like `ImageRunOptions`.

- Prompt assembly is inline in `src/llm/cards.rs` `generate_card`:

```
let prompt = format!(
    "{rendered_prompt}\n\n{ONE_SHOT_FORMAT_INSTRUCTIONS}\n\n\
     Compression target for the \"compressed\" field: {}.\n\n\
     Output ONLY valid JSON. Do not wrap in markdown code fences.",
    compression_level.oneshot_target()
);
```

`ONE_SHOT_FORMAT_INSTRUCTIONS` and `ARRAY_FORMAT_INSTRUCTIONS` are `pub(crate)`. `DEFAULT_PROMPT_TEMPLATE` is public.

- Topic fill is `crate::context::render_generation_prompt(topic, template, &CardContext)` in `src/context.rs`. `CardContext` fields are private. Add `pub(crate) fn from_parts(...)`. Do not make fields public.

- Batch prompt in `src/llm/grounding/mod.rs`:

```
pub(crate) fn batch_prompt(input: &GroundingInput<'_>) -> String {
    format!(
        "{base}\n\nWrite {count} distinct, non-overlapping cards for this load.\n\n{format}",
        base = input.rendered_prompt,
        count = batch_size(input),
        format = crate::llm::cards::ARRAY_FORMAT_INSTRUCTIONS,
    )
}
```

Extract `assemble_array_prompt(rendered_prompt, count)` next to the one-shot helper. `batch_prompt` must call it. Do not change the wording.

- `generate_card` with empty api_key returns `"Generated tip (API KEY MISSING)\n\nPrompt:\n{...}"`. Live prompt runs must NOT treat that as success.

- Repeatable UI fields (`frontend/src/components/unified_flow.rs` `TipcardInfo`, `flow_card.rs`): `tipcard_type == "repeatable_tip"`, `status`, `pinned`, `pending_count`, `review_message`. Stack layers = `pending_count.min(3)`. Cards bench is a static HTML gallery, not a Yew page.

- SM-2 (`src/scheduling/algorithms/sm2.rs`) ends with `Utc::now() + Duration::days(interval)`. Production callers:

```
// src/scheduling/mod.rs
pub fn calculate_next_review(state: &mut SchedulingState, grade: u8) -> DateTime<Utc>

// src/domain/review.rs
pub fn next_review(state: &mut SchedulingState, grade: u8) -> DateTime<Utc> {
    scheduling::calculate_next_review(state, grade)
}

// src/services/review.rs uses domain::review::next_review only
```

Keep `domain::review::next_review(state, grade)` as a production wrapper that passes `Utc::now()`, so `src/services/review.rs` does not need a signature change.

- Do not claim real FSRS. Do not add `Algorithm` variants.

# Requirements

## 1. Fix `cargo run` / `just dev`
- `default-run = "denpie"` under `[package]` in `Cargo.toml`.
- `cargo run --bin denpie` in: `justfile` `backend`, `scripts/dev.sh`, `scripts/agent-server.sh`, `benches/run_bench.sh`.
- Leave `just lab` as `--bin denpie-lab`.

## 2. Production helpers
`src/llm/cards.rs`:

```
pub(crate) fn assemble_one_shot_prompt(rendered_prompt: &str, compression_level: CompressionLevel) -> String
pub(crate) fn assemble_array_prompt(rendered_prompt: &str, count: usize) -> String
```

`generate_card` must call the one-shot helper (byte-identical prompt to today). `batch_prompt` must call the array helper.

`src/context.rs`: `pub(crate) fn from_parts` for the five title lists.

Unit test in `cards.rs`: `assemble_one_shot_prompt("Teach Rust.", CompressionLevel::Strong)` contains `ONE_SHOT_FORMAT_INSTRUCTIONS` and the Strong oneshot target `"40-70 words, or about 250-420 characters"`.

SM-2 clock:

```
pub fn calculate_next_review(state: &mut Sm2State, grade: u8, now: DateTime<Utc>) -> DateTime<Utc>
pub fn calculate_next_review(state: &mut SchedulingState, grade: u8, now: DateTime<Utc>) -> DateTime<Utc>
```

Return `now + days(interval)` instead of `Utc::now() + ...`. Update SM-2 unit tests to pass a frozen `now` (e.g. `DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z").unwrap().with_timezone(&Utc)`). Add one assertion that first-pass grade 4 returns that `now + 1 day`.

`domain::review::next_review` stays `(state, grade)` and calls `scheduling::calculate_next_review(state, grade, Utc::now())`.

## 3. Prompts bench
Default cases: `lab/cases/prompts/gold.json` (JSON array). Fields: `id` (string), `topic`, `template` (omit/empty => `DEFAULT_PROMPT_TEMPLATE`), `compression` (`light|balanced|strong|ultra`, default `strong`), `mode` (`one_shot`|`array`), `batch_count` (default 5), `existing_titles`, `dismissed_titles`, `known_items`, `difficult_items`, `uninterested_items` (arrays, default empty), `expected`.

At least:
1. English Grammar, default template, strong, one_shot, two existing titles, one of them in known_items
2. Rust, default, strong, one_shot, empty context
3. Helix Editor, default, one_shot, empty context
4. English Grammar, array, batch_count 5, at least one existing title
5. custom template containing `{topic}` and `{existing_cards}`

CLI: `run prompts --dry-run` (`--offline` alias), `--cases <path>`, live `run prompts`.

Dry-run: `render_generation_prompt` then `assemble_one_shot_prompt` or `assemble_array_prompt`. Print topic, mode, compression, prompt length, and the full assembled prompt. No `lab/runs/`. Exit 0.

Live: require non-empty `DENPIE_LAB_LLM_API_KEY` else exit 2 naming that env var. Optional `DENPIE_LAB_LLM_MODEL` (default `google/gemini-3.1-flash`), `DENPIE_LAB_LLM_BASE_URL` (default `https://openrouter.ai/api/v1`). `ReasoningConfig::new("none")`. one_shot => production `generate_card`. array => assemble only, record `kind=assembled_only`, do not call grounding. Empty-key fallback card is not a hit.

Live artifacts `lab/runs/<utc>-prompts/`: `scorecard.md`, `scorecard.json`, `cases/<id>.prompt.txt`, `cases/<id>.card.json` when generated. Columns: assembled, generated hit/miss, title_words, use_image, prompt_tokens, elapsed_ms, visual always `needs_review`. Transport miss is exit 0. Missing key is exit 2.

## 4. Cards bench (repeatable-card gallery)
Default: `lab/cases/cards/repeatable-states.json`. Fields: `id`, `topic_name`, `title`, `full_content`, `compressed_content`, `tipcard_type` (`repeatable_tip`), `status` (`active`|`reviewed`), `pinned`, `pending_count`, `review_message` (optional), `notes`.

Required ids:
- `active` unpinned, pending 0
- `pinned` pinned active
- `reviewed-hold` reviewed + review_message
- `await-refill` reviewed, no review_message, pending 0
- `daily-complete` reviewed + completion review_message
- `stacked` active, pending_count 3
- `llm-error` full_content starting with `LLM Error:`

CLI: `run cards --dry-run`, `--cases`, `run cards`.

Dry-run: print id, topic, status, pinned, pending_count, whether review_message is set. No `lab/runs/`.

`run cards`: write `lab/runs/<utc>-cards/gallery.html` and `gallery.json`. Single self-contained HTML, inline CSS, no network assets, no JS framework. One section per fixture: topic, title, badges, stack-layer count (`pending_count.min(3)` when repeatable_tip), compact vs full text. Escape `& < > "` in fixture text (no extra crate). Print the gallery path. Exit 0. No network.

## 5. Algorithms bench
Default: `lab/cases/algorithms/traces.json`.

Clock is required so replay is deterministic. Lab calls `crate::scheduling::calculate_next_review(&mut state, grade, at)` with the review timestamp, never `Utc::now()`.

Case pack JSON array. Each case:
```
{
  "id": "all-pass-one-card",
  "daily_card_count": 1,
  "reviews": [
    {"card_id": 1, "grade": 4, "at": "2025-01-01T00:00:00Z"},
    {"card_id": 1, "grade": 4, "at": "2025-01-02T00:00:00Z"},
    {"card_id": 1, "grade": 4, "at": "2025-01-08T00:00:00Z"}
  ]
}
```

Include at least:
1. `all-pass-one-card` — grades 4,4,4 at day 0, +1, +7 (SM-2 1-day then 6-day). After replay, card 1 interval must be `(6.0 * ease_after_second_pass).round()` as u32, repetitions 3.
2. `fail-reset` — pass, pass, then grade 2. repetitions 0, interval 1.
3. `deck-mix` — at least 3 card_ids, mixed grades 2 and 4, spanning >= 14 days.

CLI: `run algorithms --dry-run`, `--cases`, `run algorithms`.

Dry-run: print case ids, review counts, card ids. No `lab/runs/`.

`run algorithms` is local (no network). Replay each case twice:
- `sm2` via production `calculate_next_review(..., now=review.at)`
- `fixed_1d` a **lab-only** comparator in `src/lab/` (NOT `src/scheduling/algorithms/`): on any review, interval=1, next = at + 1 day. Do not add this to `Algorithm`.

Per case × algorithm write rows: final_reps (sum or per-card in JSON), max_same_day_due (count of reviews whose computed next_review falls on the same UTC date; or count of cards due on the busiest day after full replay — pick one, document it in the scorecard header, implement consistently), fail_reviews (grade < 3 count), mean_interval, visual `needs_review`.

Artifacts `lab/runs/<utc>-algorithms/scorecard.md` + `scorecard.json`. Print path. Exit 0.

Add a unit test that replays `all-pass-one-card` through SM-2 with frozen timestamps and asserts interval/repetitions without touching the network or `lab/runs/`.

## 6. CLI / docs / tests
- `list`: all four benches `ready`.
- USAGE documents prompts/cards/algorithms flags.
- Update `docs/lab.md` and `README.md` lab lines.
- CI-safe tests in `src/lab/` and the cards.rs/sm2 tests above:
  - prompt gold includes English Grammar and a `{existing_cards}` template
  - `run prompts --dry-run` exit 0, stdout has English Grammar and `"use_image"`
  - `run prompts` with empty `DENPIE_LAB_LLM_API_KEY` exit 2
  - cards gold has the seven ids
  - `run cards --dry-run` prints those ids
  - gallery HTML helper escapes `<script>`
  - algorithms gold loads the three traces
  - `run algorithms --dry-run` prints `all-pass-one-card`
  - SM-2 replay unit test as specified
  - existing image tests still pass
- Do not call live LLM or Bing in tests.
- Do not run live `just lab run prompts` or `just lab run images`.
- You may run `just lab run cards` and `just lab run algorithms` (local). Delete leftover `lab/runs/` dirs you create if you want; they are gitignored.

# Acceptance
Run:

```
python3 -c "import tomllib,pathlib; t=tomllib.loads(pathlib.Path('Cargo.toml').read_text()); print(t['package']['default-run'])"
grep -n 'cargo run --bin denpie' justfile scripts/dev.sh scripts/agent-server.sh benches/run_bench.sh
just lab list
just lab run prompts --dry-run
just lab run cards --dry-run
just lab run algorithms --dry-run
DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace lab -- --nocapture
DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace assemble_one_shot -- --nocapture
just quick
```

Expected:
- default-run is `denpie`
- those four files contain `cargo run --bin denpie`
- list shows four `ready` benches
- prompts dry-run prints assembled prompts including format instructions and English Grammar
- cards dry-run prints the seven fixture ids
- algorithms dry-run prints `all-pass-one-card`
- lab tests and assemble_one_shot test pass
- `just quick` passes

# Constraints
- Do not commit.
- Stay inside the workspace.
- Shell and file editor only.
- No new dependencies.
- No new `Algorithm` enum arms.
- No lab routes on the product server.
