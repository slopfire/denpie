# Task
The production helpers for denpie-lab already landed. Finish the three unimplemented benches (`prompts`, `cards`, `algorithms`) and wire them in `src/lab/mod.rs`. Do not re-extract prompt helpers or change SM-2. Do not regress the images bench (`--concurrency` stays). No HTTP routes, no protobuf, no :3017/:3027, no FSRS, no new Algorithm enum arms.

# Already done (do not redo, do use)
- workspace: /home/sfire/Projects/slopfire/denpie
- `cargo run --bin denpie` is pinned in `justfile` backend, `scripts/dev.sh`, `scripts/agent-server.sh`, `benches/run_bench.sh`. `just lab` is `--profile lab --bin denpie-lab`.
- `crate::llm::cards::assemble_one_shot_prompt` / `assemble_array_prompt` exist and are tested. `generate_card` and `batch_prompt` already call them.
- `CardContext::from_parts(existing, dismissed, known, difficult, uninterested)` is `pub(crate)` in `src/context.rs`.
- `crate::scheduling::calculate_next_review_at(state, grade, now)` and `sm2::calculate_next_review_at` exist. Frozen-now test is in `src/scheduling/algorithms/sm2.rs`.
- Images bench in `src/lab/images.rs` works. `src/lab/mod.rs` still treats algorithms/prompts/cards as planned + "not implemented". Tests `list_benches_reports_images_ready_and_three_planned` and `run_algorithms_exits_two_not_implemented` must change.
- `render_generation_prompt` is `crate::context::render_generation_prompt`.
- `DEFAULT_PROMPT_TEMPLATE` is `crate::llm::DEFAULT_PROMPT_TEMPLATE`.
- `generate_card` empty-key fallback is "Generated tip (API KEY MISSING)" — live prompts must not count that as a hit.
- Repeatable stack layers: `pending_count.min(3)` when `tipcard_type == "repeatable_tip"`.
- Case packs live under `lab/cases/`. Image pack is a JSON array at `lab/cases/images/gold.json`. `src/lab/cases.rs` currently only loads image cases; extend it or add loaders in the new modules.

# Requirements

## Prompts (`ready`)
`lab/cases/prompts/gold.json` JSON array. Fields: `id` (string), `topic`, `template` (omit/empty => DEFAULT_PROMPT_TEMPLATE), `compression` (default strong), `mode` (`one_shot`|`array`), `batch_count` (default 5), `existing_titles`/`dismissed_titles`/`known_items`/`difficult_items`/`uninterested_items` (default []), `expected`.

At least five cases:
1. English Grammar, default template, strong, one_shot, two existing titles, one of them known
2. Rust, default, strong, one_shot, empty context
3. Helix Editor, default, one_shot, empty context
4. English Grammar, array, batch_count 5, ≥1 existing title
5. Custom template containing `{topic}` and `{existing_cards}`

CLI: `run prompts --dry-run` (alias `--offline`), `--cases`.
Dry-run: `CardContext::from_parts` + `render_generation_prompt` + `assemble_one_shot_prompt` or `assemble_array_prompt`. Print topic, mode, compression, prompt length, **full assembled prompt**. No `lab/runs/`.
Live: require non-empty env `DENPIE_LAB_LLM_API_KEY` else exit 2 naming it. Optional `DENPIE_LAB_LLM_MODEL` (default `google/gemini-3.1-flash`), `DENPIE_LAB_LLM_BASE_URL` (default `https://openrouter.ai/api/v1`). `ReasoningConfig::new("none")`. `one_shot` calls production `generate_card`. `array` assemble-only (`kind=assembled_only`), no grounding. Artifacts: `lab/runs/<utc>-prompts/scorecard.md|json`, `cases/<id>.prompt.txt`, `cases/<id>.card.json` when generated. visual=`needs_review`. Miss => 0. Missing key => 2.

## Cards (`ready`)
`lab/cases/cards/repeatable-states.json` JSON array. Fields: `id`, `topic_name`, `title`, `full_content`, `compressed_content`, `tipcard_type`=`repeatable_tip`, `status`, `pinned`, `pending_count`, optional `review_message`, `notes`.
Required ids: `active`, `pinned`, `reviewed-hold`, `await-refill`, `daily-complete`, `stacked` (pending_count 3), `llm-error` (full_content starts with `LLM Error:`).
`--dry-run` prints id, topic, status, pinned, pending_count, whether review_message is set.
`run cards` writes `lab/runs/<utc>-cards/gallery.html` + `gallery.json`. Self-contained HTML, inline CSS, no network. HTML-escape text. Show stack-layer count `pending_count.min(3)` for repeatable_tip. Print path. Exit 0. No network.

## Algorithms (`ready`)
`lab/cases/algorithms/synthetic.json` **object**:
```
{
  "id": "synthetic-pass-fail",
  "daily_card_count": 3,
  "cards": ["a", "b", "c"],
  "events": [
    {"card": "a", "grade": 4, "at": "2025-01-01T10:00:00Z"}
  ]
}
```
≥3 cards, ~12 events, ISO-8601 `at`, time-ordered apply. Must include first pass (interval 1), second pass (interval 6), a third pass, a fail grade 2 that resets reps, then a pass.

`--algorithm` default `sm2`; `all` runs both:
- `sm2`: production `calculate_next_review_at` on a fresh `SchedulingState` per card
- `fixed_1d`: **only** in `src/lab/` (not `src/scheduling/`, not Algorithm enum). Pass or fail => interval 1, next = now+1 day

Scorecard: candidate, n_events, n_pass (grade>=3), n_fail, mean_final_interval, max_final_interval, fail_resets, max_due_on_any_day (max over calendar days of how many reviewed cards have next_review_at.date <= that day). Live writes `lab/runs/<utc>-algorithms/scorecard.md|json` and `timeline.json`. Dry-run prints cards, event count, date range, candidates. No network.

## CLI / docs / tests
`list_benches`: all four `ready`. Update USAGE.
Update `docs/lab.md` and `README.md` lab lines.
Modules: `src/lab/prompts.rs`, `src/lab/cards.rs`, `src/lab/algorithms.rs` (names flexible) + `mod` in `src/lab/mod.rs`. Dispatch in `run_bench`. Unknown flags still exit 2.

CI-safe tests, no network, no Postgres:
- prompt gold loads; has English Grammar and `{existing_cards}`
- `run(["run","prompts","--dry-run"])` == 0, stdout has English Grammar and `use_image`
- `run(["run","prompts"])` with DENPIE_LAB_LLM_API_KEY unset == 2
- cards gold has the seven ids; dry-run prints them
- gallery HTML helper escapes `<script>`
- algorithms pack loads; dry-run mentions sm2
- replay helper: a card with two successful grades 4 gets interval 6 under sm2
- `list` all four ready
- existing image tests still pass

Do not run live `just lab run prompts` or live images.

# Acceptance
```
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
Expected: list shows four ready; dry-runs exit 0 with the content above; algorithms and cards live write under `lab/runs/`; lab + sm2 tests pass; `just quick` passes.

# Constraints
- Do not commit. Stay in workspace. Shell + editor only. No new deps.
- Do not modify `src/scheduling/` Algorithm enum. Do not claim FSRS.
- Do not break images `--concurrency`.
