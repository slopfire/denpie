# Denpie Lab

`denpie-lab` is Denpie's opt-in research and UI-polishing workbench. It runs
production image retrieval and prompt generation, records reproducible
artifacts, compares compatible experiments, and renders production review
cards against checked-in states.

Live image and prompt runs use external providers, so they are never started
by `just test`, `just verify`, or `just ci`. All checks described as offline
below are deterministic and make no provider calls.

## Fast workflows

For card development with Astro hot reload:

```bash
just lab-cards-dev
```

Open `http://localhost:3027/lab-cards`. The toolbar filters fixtures and changes
layout, column count, viewport width, and theme. Its settings persist in the
URL, which makes a useful state easy to share or reload.

For a repeatable prompt experiment and blinded review:

```bash
just lab run prompts --tag rust --repeat 3 --label baseline
# change the prompt or implementation
just lab run prompts --tag rust --repeat 3 --label candidate
just lab-review baseline candidate
```

Open `http://localhost:3027/lab-review`, judge A/B without seeing the labels,
then reveal the labels and export `review.json`. Summarize the exported review
with:

```bash
just lab review review.json
```

The review workbench autosaves locally. It renders generated prompt cards
through the production card body and displays image artifacts directly.

## Command reference

```bash
just lab list
just lab runs
just lab show <latest|run-directory|label>
just lab label <latest|run-directory|label> <name>
just lab baseline set <name> <latest|run-directory|label>
just lab baseline show [name]

just lab run images --dry-run
just lab run images --strategy all --case grammar-basics --repeat 3 --label candidate
just lab run images --tag screenshot --concurrency 2
just lab run images --resume <run-directory>
just lab run images --resume <run-directory> --retry timeout

just lab run prompts --dry-run
just lab run prompts --case rust-empty --repeat 3 --label candidate
just lab run prompts --tag context
just lab run prompts --resume <run-directory> --retry miss

just lab run cards --dry-run
just lab run cards
just lab compare <baseline-scorecard.json> <candidate-scorecard.json>
just lab review <review.json>

just lab-cards-dev
just lab-cards-ui
just lab-cards-shot
just lab-review <baseline-run-or-scorecard> <candidate-run-or-scorecard>
just lab-check
```

`--offline` is an alias for `--dry-run`. `--case` and `--tag` are repeatable;
multiple tags use OR semantics. `--repeat` creates independent samples.
`--retry miss|timeout` is valid only with `--resume`. Image `--strategy` is
repeatable or comma-separated; `all` expands to `bing_html`,
`bing_playwright`, and `ddgs_text_og`. The default is `bing_html`.

Live jobs overlap with default concurrency 5. Playwright discovery remains
single-file. The 90-second image deadline starts after the job obtains its
concurrency permits; `queue_ms` records the wait separately. This prevents a
busy queue from being misclassified as provider failure.

Live prompt runs require `DENPIE_LAB_LLM_API_KEY`. Optional settings are
`DENPIE_LAB_LLM_MODEL` (default `google/gemini-3.1-flash`) and
`DENPIE_LAB_LLM_BASE_URL` (default `https://openrouter.ai/api/v1`). Array-mode
prompt cases are assembled and recorded as `assembled_only`; live array
generation is not wired yet.

Case IDs must match `[a-z0-9]+(-[a-z0-9]+)*`. Duplicate and path-like IDs are
rejected before artifacts are created. Unknown commands, flags, and benches
exit 2.

## Benches

| Bench        | Status  | Purpose                                                                      |
| ------------ | ------- | ---------------------------------------------------------------------------- |
| `images`     | ready   | Exercise production `retrieve_image` over the five-card gold set.            |
| `prompts`    | ready   | Assemble prompt cases and run production `generate_card` for one-shot cases. |
| `cards`      | ready   | Generate an offline catalog from the checked-in production-card fixtures.    |
| `algorithms` | planned | Future scheduler experiments. Production scheduling remains SM-2.            |

## Reproducible runs

Every new image or prompt run starts by writing `manifest.json`. It records the
manifest schema, bench, label, UTC start time, Git revision and dirty state,
case-pack path and content hash, selected model/API origin without credentials,
compatibility inputs, and execution settings. Scorecards are atomically
checkpointed after each result.

`--resume` first checks the manifest and refuses incompatible inputs. Completed
rows are reused; missing rows continue. `--retry miss` reruns misses, while
`--retry timeout` reruns only timeouts. Run directories use sub-millisecond
timestamps plus a collision suffix, so concurrent processes do not share
output accidentally.

`just lab runs`, `show`, and `label` make artifacts discoverable without
copying timestamped paths. Named baseline mappings live in ignored
`lab/runs/baselines.json`. The review workbench accepts a direct run/scorecard
path, run directory name, run label, `latest`, or named baseline.

### Image output

```text
lab/runs/<utc-timestamp>-images/
├── manifest.json
├── scorecard.md
├── scorecard.json
└── cases/<case-id>/<strategy>-<repeat-index>.<ext>
```

Rows contain case, repeat, strategy, outcome kind, byte/MIME/extension data,
`elapsed_ms`, `queue_ms`, and `failure_stage`. Summaries report median and p95
latency. A miss or timeout is evidence and does not make the command fail.
Visual quality remains `needs_review`; file size is never treated as quality.

### Prompt output

```text
lab/runs/<utc-timestamp>-prompts/
├── manifest.json
├── scorecard.md
├── scorecard.json
└── cases/
    ├── <case-id>-<repeat-index>.prompt.txt
    └── <case-id>-<repeat-index>.card.json
```

Rows include repeat, assembly/generation outcome, content word counts, image
intent, token counts, elapsed time, and a redacted error. Summaries report
median/p95 latency and total tokens. The assembled prompt is written before
generation so a failed call still leaves a diagnostic artifact. Stored errors
remove credentials and URL query values.

## Comparing and reviewing runs

`just lab compare` matches image rows by case/strategy/repeat and prompt rows
by case/repeat. It reports added and removed rows, mechanical outcome changes,
and signed deltas for latency, bytes, word counts, and tokens. When both runs
have manifests, incompatible experiment inputs are rejected. Legacy
scorecards without manifests remain readable.

Mechanical comparison answers whether behavior changed. `just lab-review`
answers whether it improved. The workbench randomizes baseline/candidate into
A/B deterministically per pair and records verdicts for overall quality,
correctness, learnability, compression, image relevance, and UI fit. Keyboard
shortcuts `1`, `2`, and `3` set the overall verdict to A, tie, or B. Exported
reviews keep stable run identities and can be validated and summarized with
`just lab review`.

## Production card UI lab

`lab/cases/cards/repeatable-states.json` contains 15 fixtures covering active,
pinned, reviewed/hold, refill, completion, stacking, LLM error, long Markdown,
one through three images, broken images, missing API key, and repeatable/manual/
custom/casual card types. Optional fixture metadata covers timestamps, repeat
count, topic icon/color, and sources. The parser enforces the production
four-image limit.

`just lab-cards-dev` serves `/lab-cards` with HMR on agent port `:3027`.
`just lab-cards-ui` builds and serves the same page without HMR. Fixtures pass
through the production `ReviewSlotCard`, card bodies, detail view, lightbox,
and review actions. Pin, delete, review, and Continue use deterministic local
adapters; detail loading uses an offline fixture adapter. The page performs no
API writes.

`just lab run cards` writes a self-contained `gallery.html` and `gallery.json`
under `lab/runs/`. That lightweight static catalog is useful for inspection,
but it is not the production React rendering.

`just lab-cards-shot` is an opt-in screenshot matrix across narrow/wide and
light/dark states. It uses isolated port `:3027` and requires the local browser
dependencies used by Playwright.

## Offline contract check

`just lab-check` is the rerunnable proof for lab development. It covers CLI
plans and invalid inputs, fixtures, manifests/resume/comparison/review logic,
frontend unit tests, Astro builds, hydrated production-card interactions,
polish controls, and review export. It never starts a live provider run and
does not write experimental results under `lab/runs/`.
