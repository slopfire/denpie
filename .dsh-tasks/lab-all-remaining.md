# Task
Finish denpie-lab. Pin every server `cargo run` to `--bin denpie`. Implement the `prompts`, `cards`, and `algorithms` benches so `just lab list` shows all four as `ready`. Keep calling production code. No HTTP routes, no protobuf, no bind of :3017/:3027, no real FSRS.

# Repository facts
- workspace: /home/sfire/Projects/slopfire/denpie
- `Cargo.toml` already has `default-run = "denpie"` and `[profile.lab]`. Do **not** remove `default-run`. Restore `rust-version = "1.95.0"` under `[package]` if it is missing (the original package stanza had it).
- `just lab` is: `DENPIE_SKIP_FRONTEND_BUILD=1 cargo run --profile lab --bin denpie-lab -- {{args}}`
- Images bench already works (`src/lab/images.rs`, `--concurrency`, gold set). Do not regress it. Keep `ImageRunOptions` / concurrency.
- `src/lab/mod.rs` still routes `algorithms|prompts|cards` to "not implemented" and lists them `planned`. A test named `list_benches_reports_images_ready_and_three_planned` and `run_algorithms_exits_two_not_implemented` must be updated.
- Server start sites still omit `--bin denpie` (ambiguous-safe now because of default-run, but pin them anyway):
  - `justfile` recipe `backend`: `DENPIE_SKIP_FRONTEND_BUILD=1 cargo run`
  - `scripts/dev.sh` line 14: `DENPIE_SKIP_FRONTEND_BUILD=1 cargo run &`
  - `scripts/agent-server.sh`: `cargo run >"$LOG_FILE" 2>&1 &`
  - `benches/run_bench.sh`: `cargo run &`
- Prompt assembly is inline in `src/llm/cards.rs` `generate_card`:

```
let prompt = format!(
    "{rendered_prompt}\n\n{ONE_SHOT_FORMAT_INSTRUCTIONS}\n\n\
     Compression target for the \"compressed\" field: {}.\n\n\
     Output ONLY valid JSON. Do not wrap in markdown code fences.",
    compression_level.oneshot_target()
);
```

`ONE_SHOT_FORMAT_INSTRUCTIONS` / `ARRAY_FORMAT_INSTRUCTIONS` are `pub(crate)`. `DEFAULT_PROMPT_TEMPLATE` is public.

- `crate::context::render_generation_prompt(topic, template, &CardContext)` builds the topic text. `CardContext` fields are private. Add `pub(crate) fn from_parts(...)`. Do not make fields public.

- Batch prompt in `src/llm/grounding/mod.rs` `batch_prompt`:

```
format!(
    "{base}\n\nWrite {count} distinct, non-overlapping cards for this load.\n\n{format}",
    base = input.rendered_prompt,
    count = batch_size(input),
    format = crate::llm::cards::ARRAY_FORMAT_INSTRUCTIONS,
)
```

Extract `assemble_array_prompt(rendered_prompt, count)` next to the one-shot helper. `batch_prompt` must call it. Do not change wording.

- `generate_card` with empty api_key returns "Generated tip (API KEY MISSING)". Live prompt runs must not treat that as success.

- Repeatable card states: `TipcardInfo` in `frontend/src/components/unified_flow.rs`. Fields: id, topic_name, title, full_content, compressed_content, tipcard_type=`repeatable_tip`, status, pinned, pending_count, review_message. Stack layers = `pending_count.min(3)` for repeatable_tip. Cards bench is a static HTML gallery, not a Yew page.

- Scheduling today (`src/scheduling/algorithms/sm2.rs`):

```
pub fn calculate_next_review(state: &mut Sm2State, grade: u8) -> DateTime<Utc> {
    ...
    Utc::now() + chrono::Duration::days(state.interval as i64)
}
```

`src/scheduling/mod.rs` `calculate_next_review(state, grade)` dispatches SM2 only. `FSRS` is a serde alias. `src/domain/review.rs` `next_review` and `src/services/review.rs` call that. Production algorithm enum stays SM2-only.

- Image gold set path: `lab/cases/images/gold.json`. Copy the JSON-array case-pack style.

# Requirements

## 1. Pin server `cargo run`
Change the four server start sites to `cargo run --bin denpie` (keep existing env flags). Leave `just lab` as `--bin denpie-lab`. Do not change ports.

## 2. Production helpers
`src/llm/cards.rs`:
```
pub(crate) fn assemble_one_shot_prompt(rendered_prompt: &str, compression_level: CompressionLevel) -> String
pub(crate) fn assemble_array_prompt(rendered_prompt: &str, count: usize) -> String
```
`generate_card` must call the one-shot helper (byte-identical prompt). `batch_prompt` must call the array helper.

`src/context.rs`: `pub(crate)` constructor for `CardContext` from the five title vecs.

Clock for replay, keep old signatures working:
```
// sm2.rs
pub fn calculate_next_review(state: &mut Sm2State, grade: u8) -> DateTime<Utc> {
    calculate_next_review_at(state, grade, Utc::now())
}
pub fn calculate_next_review_at(state: &mut Sm2State, grade: u8, now: DateTime<Utc>) -> DateTime<Utc>
```
Same pattern on `src/scheduling/mod.rs` (`calculate_next_review` still `Utc::now()`, plus `calculate_next_review_at`). Existing SM-2 tests keep compiling. Add one test that a frozen `now` yields `now + interval days`. Do not add Algorithm arms. Do not claim FSRS.

Unit test: `assemble_one_shot_prompt("Teach Rust.", CompressionLevel::Strong)` contains `ONE_SHOT_FORMAT_INSTRUCTIONS` and the Strong oneshot_target string.

## 3. Prompts bench (`ready`)
Default `lab/cases/prompts/gold.json` (JSON array). Fields: `id` (string), `topic`, `template` (omit/empty => DEFAULT_PROMPT_TEMPLATE), `compression` (default strong), `mode` (`one_shot`|`array`), `batch_count` (default 5), `existing_titles`, `dismissed_titles`, `known_items`, `difficult_items`, `uninterested_items` (default []), `expected`.

At least:
1. English Grammar, default template, strong, one_shot, two existing titles, one known
2. Rust, default, strong, one_shot, empty context
3. Helix Editor, default, one_shot, empty context
4. English Grammar, array, batch_count 5, at least one existing title
5. Custom template containing `{topic}` and `{existing_cards}`

CLI like images: `--dry-run`/`--offline`, `--cases`.
Dry-run: `render_generation_prompt` then assemble one-shot or array. Print topic, mode, compression, prompt length, **full assembled prompt**. No `lab/runs/`.
Live: require non-empty `DENPIE_LAB_LLM_API_KEY` else exit 2 naming that var. Optional `DENPIE_LAB_LLM_MODEL` (default `google/gemini-3.1-flash`), `DENPIE_LAB_LLM_BASE_URL` (default `https://openrouter.ai/api/v1`). `ReasoningConfig::new("none")`. `one_shot` => production `generate_card`. `array` => assemble only, `kind=assembled_only`, no fake grounding. Do not count empty-key fallback as a hit.
Artifacts: `lab/runs/<utc>-prompts/scorecard.md|json`, `cases/<id>.prompt.txt`, `cases/<id>.card.json` when generated.
Columns: assembled, generated hit/miss, title_words, use_image, prompt_tokens, elapsed_ms, visual=`needs_review`. Generation miss => exit 0. Missing key => 2.

## 4. Cards bench (`ready`)
Default `lab/cases/cards/repeatable-states.json`. Repeatable-card UI fixtures, no network.
Fields: `id` (string), `topic_name`, `title`, `full_content`, `compressed_content`, `tipcard_type`=`repeatable_tip`, `status`, `pinned`, `pending_count`, `review_message` optional, `notes`.
Required ids:
1. `active` unpinned active pending 0
2. `pinned` pinned active
3. `reviewed-hold` reviewed + review_message
4. `await-refill` reviewed, no review_message, pending 0
5. `daily-complete` reviewed + completion review_message
6. `stacked` active pending_count 3
7. `llm-error` full_content starts with `LLM Error:`

`--dry-run` prints id, topic, status, pinned, pending_count, whether review_message is set. No runs dir.
`run cards` writes `lab/runs/<utc>-cards/gallery.html` and `gallery.json`. One self-contained HTML file, inline CSS, no network assets. One section per fixture: badges, stack-layer count (`pending_count.min(3)` if repeatable_tip), compact vs full text. HTML-escape fixture text. Print gallery path. Exit 0.

## 5. Algorithms bench (`ready`)
Default `lab/cases/algorithms/synthetic.json`.
This is a **replay**, not a new production scheduler.

Case pack shape (object, not array):
```
{
  "id": "synthetic-pass-fail",
  "daily_card_count": 3,
  "cards": ["a", "b", "c"],
  "events": [
    {"card": "a", "grade": 4, "at": "2025-01-01T10:00:00Z"},
    ...
  ]
}
```
Include at least: first-pass (interval 1), second-pass (interval 6), a third pass, a fail (grade 2) that resets reps, then a pass again. At least 3 cards and ~12 events with ISO-8601 `at` values spanning multiple days. Sort/apply events in time order.

Candidates (CLI `--algorithm`, default `sm2`, `all` runs both):
- `sm2`: production `crate::scheduling::calculate_next_review_at` on a fresh `SchedulingState` per card.
- `fixed_1d`: **lab-only** in `src/lab/`, not in `src/scheduling/`, not an `Algorithm` enum arm. Pass or fail sets interval=1, next = now+1 day. Exists so the scorecard has two columns.

Replay independently per candidate. Per card: apply each of its events at `at` as `now`. Record next_review_at, interval, repetitions, ease_factor (ease only meaningful for sm2; fixed_1d can omit or use 0).

Scorecard mechanical columns (no visual quality column needed; use `notes` if you want):
- candidate
- n_events, n_pass (grade>=3), n_fail (grade<3)
- mean_final_interval, max_final_interval
- fail_resets (times repetitions went to 0 after a fail)
- max_due_on_any_day: for each calendar day from min(at) to max(next_review), count cards whose next_review_at date is <= that day and who have been reviewed at least once; take the max. This is the load number to compare against `daily_card_count`.

Dry-run: print cards, event count, date range, candidates. No runs dir.
Live (no network): `lab/runs/<utc>-algorithms/scorecard.md|json` plus `timeline.json` (per candidate, per card, list of {at, grade, interval, next_review_at}). Exit 0.

## 6. CLI / docs / tests
`list`: images, prompts, cards, algorithms all `ready`.
Update USAGE for all new flags.
Update `docs/lab.md` and the lab lines in `README.md`.
CI-safe tests (`#[cfg(test)]`, no network, no Postgres):
- prompt gold loads; includes English Grammar and a `{existing_cards}` template
- `run prompts --dry-run` exit 0, stdout has English Grammar and a distinctive format-instruction substring (`use_image`)
- `run prompts` with DENPIE_LAB_LLM_API_KEY unset/empty exit 2
- cards gold has the seven ids; dry-run prints them
- gallery HTML helper escapes `<script>`
- algorithms pack loads; dry-run prints sm2 and the event count
- `run algorithms` (live, no network) exit 0; sm2 second successful review on a card that had two passes reaches interval 6 (assert via the replay helper, not by reading lab/runs if you prefer a direct function test)
- frozen-now SM-2 unit test
- existing image tests still pass
- `list` all four ready

Do not run live `just lab run prompts` (needs a real key).
Do not run live images.
`just lab run cards` and `just lab run algorithms` are local and may be used in acceptance; delete any `lab/runs/` leftovers you created if you want, they are gitignored.

# Acceptance
Run exactly:

```
python3 -c "import tomllib,pathlib; t=tomllib.loads(pathlib.Path('Cargo.toml').read_text()); print(t['package']['default-run']); print(t['package'].get('rust-version',''))"
grep -n 'cargo run' justfile scripts/dev.sh scripts/agent-server.sh benches/run_bench.sh
just lab list
just lab run prompts --dry-run
just lab run cards --dry-run
just lab run algorithms --dry-run
just lab run algorithms
just lab run cards
DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace lab -- --nocapture
just test-one test_sm2
just quick
```

Expected:
- default-run is `denpie`; rust-version is `1.95.0`
- server start sites include `--bin denpie`; `just lab` still `--bin denpie-lab`
- `just lab list` shows four benches `ready`
- prompts dry-run prints assembled prompts (format instructions + English Grammar)
- cards dry-run prints the seven fixture ids
- algorithms dry-run and live exit 0; live writes a scorecard under `lab/runs/`
- cards live writes gallery.html
- lab tests + sm2 tests pass
- `just quick` passes

# Constraints
- Do not commit.
- Stay inside the workspace.
- Shell and file editor only.
- No new crates/deps.
- Do not add production Algorithm variants. Do not claim real FSRS.
- Do not add lab routes to the product server.
- Do not break the images bench or `--concurrency`.
