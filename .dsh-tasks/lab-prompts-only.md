# Task
Implement only the `prompts` lab bench. Leave `cards` and `algorithms` as planned. Do not touch `src/scheduling/`. Do not break images.

# Facts
workspace: /home/sfire/Projects/slopfire/denpie

Helpers already exist:
- `crate::llm::cards::assemble_one_shot_prompt(rendered, compression)`
- `crate::llm::cards::assemble_array_prompt(rendered, count)`
- `crate::context::render_generation_prompt(topic, template, &CardContext)`
- `CardContext::from_parts(existing, dismissed, known, difficult, uninterested)`
- `crate::llm::DEFAULT_PROMPT_TEMPLATE`
- `crate::llm::generate_card`
- `crate::llm::ReasoningConfig::new("none")`
- empty api_key => generate_card returns "Generated tip (API KEY MISSING)" — not a live hit

`src/lab/mod.rs` currently marks prompts planned and `run prompts` exits 2. Images `--concurrency` must keep working.

# Do
1. `src/lab/prompts.rs` + `mod prompts;` Dispatch `run prompts` like images.
2. `lab/cases/prompts/gold.json` JSON array with ≥5 cases:
   - English Grammar, default template, strong, one_shot, two existing titles one known
   - Rust, default, strong, one_shot, empty context
   - Helix Editor, default, one_shot, empty
   - English Grammar, mode array, batch_count 5, ≥1 existing title
   - custom template containing `{topic}` and `{existing_cards}`
   Fields: id (string), topic, optional template, compression (default strong), mode (`one_shot`|`array`), batch_count (default 5), title arrays default [], expected.
3. `--dry-run`/`--offline`, `--cases`. Dry-run prints topic, mode, compression, prompt length, full assembled prompt. No lab/runs.
4. Live: require `DENPIE_LAB_LLM_API_KEY` else exit 2 naming it. Optional `DENPIE_LAB_LLM_MODEL` default `google/gemini-3.1-flash`, `DENPIE_LAB_LLM_BASE_URL` default `https://openrouter.ai/api/v1`. one_shot => generate_card. array => assemble only kind=assembled_only. Write `lab/runs/<utc>-prompts/scorecard.md|json`, `cases/<id>.prompt.txt`, `cases/<id>.card.json` if generated. visual=needs_review.
5. `list_benches`: prompts `ready`; cards and algorithms stay `planned`; images stays ready.
6. Update USAGE and `docs/lab.md` prompts section only.
7. Tests (no network): gold loads; dry-run exit 0 contains English Grammar and `use_image`; live without key exit 2.

Do not run live prompts (needs a real key). Do not commit.

# Acceptance
```
just lab list
just lab run prompts --dry-run
just lab run cards; echo cards_exit:$?
DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace lab -- --nocapture
just quick
```
list shows prompts ready, cards/algorithms planned; prompts dry-run prints assembled prompts; run cards still exits 2; lab tests pass; just quick passes.
