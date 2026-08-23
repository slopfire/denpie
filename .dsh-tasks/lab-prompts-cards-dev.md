# Task
Fix `just dev` / `cargo run` (ambiguous now that `denpie-lab` exists) and implement the `prompts` and `cards` lab benches. Leave `algorithms` planned. Do not add HTTP routes, protobuf ops, or bind :3017/:3027.

# Repository facts
- workspace: /home/sfire/Projects/slopfire/denpie
- Current failure (verbatim):

```
error: `cargo run` could not determine which binary to run. Use the `--bin` option to specify a binary, or the `default-run` manifest key.
available binaries: denpie, denpie-lab
```

This breaks `just backend`, `just dev` (`scripts/dev.sh` line 14: `DENPIE_SKIP_FRONTEND_BUILD=1 cargo run &`), `scripts/agent-server.sh` (`cargo run >"$LOG_FILE"`), and `benches/run_bench.sh` (`cargo run &`). README still documents `cargo run`.

- Lab CLI lives in `src/lab/mod.rs`. `run_bench` currently:

```
"algorithms" | "prompts" | "cards" => {
    usage_error(stderr, &format!("bench `{bench}` is not implemented"))
}
```

`list_benches()` marks `images` ready and the other three planned. Tests in `src/lab/mod.rs` assert prompts/cards are planned — update those tests.

- Prompt assembly today is inline in `src/llm/cards.rs` `generate_card`:

```
let prompt = format!(
    "{rendered_prompt}\n\n{ONE_SHOT_FORMAT_INSTRUCTIONS}\n\n\
     Compression target for the \"compressed\" field: {}.\n\n\
     Output ONLY valid JSON. Do not wrap in markdown code fences.",
    compression_level.oneshot_target()
);
```

`ONE_SHOT_FORMAT_INSTRUCTIONS` and `ARRAY_FORMAT_INSTRUCTIONS` are `pub(crate)` in that file. `DEFAULT_PROMPT_TEMPLATE` is public.

- Topic template → model-facing text is `crate::context::render_generation_prompt(topic, template, &CardContext)` in `src/context.rs`. `CardContext` fields are private; tests in that module use struct literals. Lab is the same crate but a different module — add a `pub(crate)` constructor, do not make fields public.

```
pub struct CardContext {
    existing_titles: Vec<String>,
    dismissed_titles: Vec<String>,
    known_items: Vec<String>,
    difficult_items: Vec<String>,
    uninterested_items: Vec<String>,
}
```

- Batch/array prompt is `pub(crate) fn batch_prompt` in `src/llm/grounding/mod.rs`:

```
format!(
    "{base}\n\nWrite {count} distinct, non-overlapping cards for this load.\n\n{format}",
    base = input.rendered_prompt,
    count = batch_size(input),
    format = crate::llm::cards::ARRAY_FORMAT_INSTRUCTIONS,
)
```

Extract a helper that does not require `GroundingInput` so the lab can call it. Keep `batch_prompt` as a thin wrapper (do not duplicate the format string).

- `generate_card` (`src/llm/cards.rs`) is the live one-shot path. Empty `api_key` returns a "Generated tip (API KEY MISSING)" fallback — **live prompt runs must not treat that as success**. If the API key env is missing, exit 2.

- Repeatable card UI states live in `frontend/src/components/unified_flow.rs` (`TipcardInfo`) and `frontend/src/components/flow_card.rs`. Relevant fields: `id`, `topic_name`, `title`, `full_content`, `compressed_content`, `tipcard_type` (`repeatable_tip`), `status`, `pinned`, `pending_count`, `review_message`. Stack layers: `pending_count.min(3)` for `repeatable_tip`. Do not add a Yew/WASM lab page or a dashboard route. The cards bench is a **static HTML fixture gallery** plus JSON, for redesign inspection.

- Image bench pattern to copy: `src/lab/images.rs`, `lab/cases/images/gold.json`, `--dry-run` prints a plan and writes nothing under `lab/runs/`, live writes `lab/runs/<utc>-<bench>/`. No clap. No new crates.

- Commands that already work: `just lab list`, `just lab run images --dry-run`, `just quick`.

# Requirements

## 1. Fix `cargo run` / `just dev`
- Add `default-run = "denpie"` under `[package]` in `Cargo.toml` so bare `cargo run` starts the server again.
- Also pass `--bin denpie` at every place that starts the **server**:
  - `justfile` `backend` recipe
  - `scripts/dev.sh`
  - `scripts/agent-server.sh`
  - `benches/run_bench.sh`
- Leave `just lab` as `cargo run --bin denpie-lab`.
- Do not change ports. `:3017` remains the user/dev server; agents still use `:3027` via agent-server.

## 2. Production helpers the lab must call (no copies)
In `src/llm/cards.rs`, extract and use from `generate_card`:

```
pub(crate) fn assemble_one_shot_prompt(
    rendered_prompt: &str,
    compression_level: CompressionLevel,
) -> String
```

The returned string must be **exactly** what `generate_card` sends to the model today (same format instructions, compression target line, "Output ONLY valid JSON…" suffix).

In `src/llm/cards.rs` (or grounding, but prefer cards.rs so ARRAY stays next to ONE_SHOT), extract:

```
pub(crate) fn assemble_array_prompt(rendered_prompt: &str, count: usize) -> String
```

`batch_prompt` must call this. Do not change batch wording.

In `src/context.rs`, add `pub(crate) fn from_parts(...)` (or equivalent) so lab can build a `CardContext` from case-pack title lists.

Add a small unit test in `cards.rs` that `assemble_one_shot_prompt("Teach Rust.", CompressionLevel::Strong)` contains `ONE_SHOT_FORMAT_INSTRUCTIONS` and the Strong `oneshot_target()` text.

## 3. Prompts bench
- Status `ready`. Default cases: `lab/cases/prompts/gold.json`.
- Case fields (JSON array): `id` (string), `topic`, `template` (string or omit/empty to use `DEFAULT_PROMPT_TEMPLATE`), `compression` (`light|balanced|strong|ultra`, default `strong`), `mode` (`one_shot` or `array`), `batch_count` (used when mode=array, default 5), `existing_titles`, `dismissed_titles`, `known_items`, `difficult_items`, `uninterested_items` (arrays, default empty), `expected` (human rubric string).
- Include at least these cases:
  1. English Grammar, default template, strong, one_shot, two existing titles one of which is known
  2. Rust, default template, strong, one_shot, empty context
  3. Helix Editor, default template, one_shot, empty context
  4. English Grammar, array mode, batch_count 5, at least one existing title
  5. A custom template that contains `{topic}` and `{existing_cards}`
- CLI (same argv style as images):
  - `run prompts --dry-run` (alias `--offline`)
  - `run prompts --cases <path>`
  - `run prompts` live
- Dry-run: load cases, for each case call `render_generation_prompt` then `assemble_one_shot_prompt` or `assemble_array_prompt`. Print topic, mode, compression, prompt length, and the **full assembled prompt**. Write nothing under `lab/runs/`. Exit 0.
- Live: require env `DENPIE_LAB_LLM_API_KEY` (non-empty). Optional `DENPIE_LAB_LLM_MODEL` (default `google/gemini-3.1-flash`) and `DENPIE_LAB_LLM_BASE_URL` (default `https://openrouter.ai/api/v1`). `ReasoningConfig::new("none")`. For `one_shot` call production `generate_card`. For `array`, still assemble the array prompt and **skip the LLM call** for that case (record `kind=assembled_only` / note that live array generation is not wired) — do not invent a new grounding run. If the key is missing, exit 2 with a message naming `DENPIE_LAB_LLM_API_KEY`. Do not count the empty-key fallback card as a hit.
- Live artifacts under `lab/runs/<utc>-prompts/`:
  - `scorecard.md` + `scorecard.json`
  - `cases/<id>.prompt.txt` (assembled prompt)
  - `cases/<id>.card.json` when a card was generated
- Mechanical columns: `assembled` always true on success; `generated` hit/miss; `title_words`; `use_image`; `prompt_tokens`; `elapsed_ms`. `visual` always `needs_review`. Exit 0 if generation misses (transport error recorded as miss) **except** missing API key (exit 2).

## 4. Cards bench (repeatable-card fixture gallery)
- Status `ready`. Default cases: `lab/cases/cards/repeatable-states.json`.
- This is for redesigning the repeatable card, not LLM quality. No network.
- JSON array of fixtures with at least: `id` (string), `topic_name`, `title`, `full_content`, `compressed_content`, `tipcard_type` (use `repeatable_tip`), `status` (`active` or `reviewed`), `pinned` (bool), `pending_count` (u32), `review_message` (optional string), `notes` (why this state exists).
- Required fixtures (names can be the `id`s):
  1. `active` — unpinned active repeatable, pending_count 0
  2. `pinned` — pinned active
  3. `reviewed-hold` — reviewed + `review_message` set (holds the topic slot)
  4. `await-refill` — reviewed, no review_message, pending_count 0 (waiting for next card)
  5. `daily-complete` — reviewed placeholder with a completion `review_message`
  6. `stacked` — active with `pending_count` 3 (stack layers)
  7. `llm-error` — `full_content` starting with `LLM Error:` (FlowCard error kind)
- CLI: `run cards --dry-run`, `run cards --cases <path>`, `run cards`.
- Dry-run: print each fixture id, topic, status, pinned, pending_count, whether review_message is set. No `lab/runs/`.
- `run cards` (not dry-run): write `lab/runs/<utc>-cards/gallery.html` and `gallery.json`. HTML must be a single self-contained file (inline CSS, no network assets, no JS framework). One section per fixture showing topic, title, badges for status/pinned/pending/review, stack-layer count (`pending_count.min(3)` when type is repeatable_tip), and the compact vs full text. Escape HTML in user/fixture text. Print the gallery path. Exit 0.

## 5. CLI / docs / tests
- `list` must show `images`, `prompts`, `cards` as `ready` and `algorithms` as `planned`.
- Update USAGE help for the new flags.
- `run algorithms` still exits 2 "not implemented".
- Update `docs/lab.md`, `README.md` lab lines, and any bench table copy.
- CI-safe tests only (`#[cfg(test)]` in `src/lab/`):
  - prompt gold pack loads and includes English Grammar + a `{existing_cards}` template case
  - `run(["run","prompts","--dry-run"])` returns 0 and stdout contains `ONE_SHOT_FORMAT_INSTRUCTIONS` text (or a distinctive substring from it, e.g. `"use_image"`) and `English Grammar`
  - `run(["run","prompts"])` with `DENPIE_LAB_LLM_API_KEY` unset/empty returns 2
  - cards gold pack contains the seven fixture ids above
  - `run(["run","cards","--dry-run"])` returns 0 and prints those ids
  - gallery HTML helper escapes `<script>` in title/content
  - `list` statuses as specified
  - existing image tests still pass
- Do not call live LLM or Bing in tests.
- Do not run a live `just lab run prompts` (needs a real key).
- `just lab run cards` **may** be run in verification because it is local; if you run it, delete any `lab/runs/` leftovers you created (gitignored anyway).

# Acceptance
Run exactly:

```
DENPIE_SKIP_FRONTEND_BUILD=1 cargo run --bin denpie -- --help >/dev/null; echo cargo_bin_denpie:$?
# Prove default-run is the server binary, not denpie-lab. Do NOT leave a server running.
# `cargo run` with no args starts the server and needs DATABASE_URL; instead:
python3 -c "import tomllib,pathlib; t=tomllib.loads(pathlib.Path('Cargo.toml').read_text()); print(t['package']['default-run'])"
just lab list
just lab run prompts --dry-run
just lab run cards --dry-run
just lab run algorithms; echo algorithms_exit:$?
DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace lab -- --nocapture
just quick
```

Expected:
- `Cargo.toml` package.default-run prints `denpie`
- `just lab list` shows prompts and cards `ready`, algorithms `planned`
- prompts dry-run exits 0, prints assembled prompts including format instructions and English Grammar
- cards dry-run exits 0, prints the seven fixture ids
- `just lab run algorithms` exits 2
- lab tests pass with no network
- `just quick` passes
- `scripts/dev.sh` and `justfile` backend use `--bin denpie`

# Constraints
- Do not commit.
- Stay inside the workspace.
- Use the shell and file editor only.
- No new dependencies.
- SM-2 remains the only production scheduler; do not implement the algorithms bench.
- Do not add lab routes to the product server.
