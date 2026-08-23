# Task
Implement only the `cards` lab bench (repeatable-card fixture gallery). Leave `algorithms` planned. Do not break `images` or `prompts`. No HTTP, no Yew page, no network.

# Facts
workspace: /home/sfire/Projects/slopfire/denpie

`src/lab/mod.rs` already has images+prompts ready. `run cards` still exits 2 "not implemented". Copy the CLI style of `src/lab/prompts.rs` (dry-run, --cases, run_with dispatch).

Repeatable UI fields from TipcardInfo: topic_name, title, full_content, compressed_content, tipcard_type=`repeatable_tip`, status, pinned, pending_count, review_message. Stack layers = pending_count.min(3) when type is repeatable_tip.

# Do
1. `src/lab/cards.rs` + `mod cards;` Dispatch `run cards`.
2. `lab/cases/cards/repeatable-states.json` JSON array with exactly these ids:
   - `active` unpinned active pending 0
   - `pinned` pinned active
   - `reviewed-hold` reviewed + review_message
   - `await-refill` reviewed, no review_message, pending 0
   - `daily-complete` reviewed + completion review_message
   - `stacked` active pending_count 3
   - `llm-error` full_content starts with `LLM Error:`
   Also: topic_name, title, full_content, compressed_content, tipcard_type, status, pinned, pending_count, optional review_message, notes.
3. `--dry-run`/`--offline`, `--cases`. Dry-run prints id, topic, status, pinned, pending_count, whether review_message is set. No lab/runs.
4. `run cards` writes `lab/runs/<utc>-cards/gallery.html` and `gallery.json`. Self-contained HTML, inline CSS, no network assets, no JS framework. HTML-escape all fixture text (a helper must escape `<script>`). One section per fixture with badges and stack-layer count. Print gallery path. Exit 0.
5. `list_benches`: cards `ready`. algorithms stays `planned`.
6. Update USAGE and `docs/lab.md` cards section.
7. Tests no network: gold has the seven ids; dry-run prints them; gallery helper escapes `<script>`.

Do not commit. Do not implement algorithms.

# Acceptance
```
just lab list
just lab run cards --dry-run
just lab run cards
just lab run algorithms; echo algorithms_exit:$?
DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace lab -- --nocapture
just quick
```
list shows cards ready, algorithms planned; dry-run prints the seven ids; live writes gallery.html; algorithms still exit 2; tests + just quick pass.
