# Task
Create `denpie-lab`: an opt-in research runner that shares production code. First closed unit is (1) split the binary crate into `src/lib.rs` + thin `src/main.rs`, (2) add `src/bin/denpie-lab.rs` and a `just lab` recipe, (3) implement `list` plus a real `images` bench over the existing five-card gold set that calls production `retrieve_image`, and (4) document it. Do not add lab HTTP routes, protobuf ops, or anything that binds :3017/:3027.

# Repository facts
- workspace: /home/sfire/Projects/slopfire/denpie
- This is a binary-only crate today: `src/main.rs` owns every `mod` and `#[cfg(test)] mod tests;`. There is no `src/lib.rs`.
- Tests live in `src/tests/` and use `crate::...` (see `src/tests/support.rs`, `src/tests/image_enrichment.rs`). After the split they must stay crate-private unit tests on the **library**, not a new `tests/` integration crate.
- Image retrieval already exists and is the extension point:

```
// src/llm/images/mod.rs
pub async fn retrieve_image(strategy: ImageStrategy, input: ImageInput<'_>) -> Option<RetrievedImage>

pub struct ImageInput<'a> {
    pub topic_name: &'a str,
    pub card_title: &'a str,
    pub card_content: &'a str,
    pub image_query: &'a str,
    pub model: &'a str,
    pub api_key: &'a str,
    pub api_base: &'a str,
    pub reasoning: &'a ReasoningConfig,
    pub pool: &'a [PoolImageMeta],
}

pub enum RetrievedImage {
    Prepared(crate::image_compress::PreparedImage), // bytes, mime_type, extension
    Pool(i64),
}
```

- `ImageStrategy` in `src/domain/grounding.rs`: `None`, `Pool`, `BingHtml` (`bing_html`), `BingPlaywright` (`bing_playwright`), `DdgsTextOg` (`ddgs_text_og`). `from_setting` / `as_str` already exist.
- Re-exports already in `src/llm/mod.rs`: `ImageInput`, `RetrievedImage`, `retrieve_image`, `ReasoningConfig`.
- `ReasoningConfig::new("none")` is enough for image fetch (Bing/DDG do not need an LLM).
- `src/main.rs` current `main` starts the HTTP server (default bind `127.0.0.1:3017`), builds frontend, opens Postgres. Move that body into `pub async fn run()` on the library. The lab binary must **not** call `run()`.
- Current `src/main.rs` header (keep these module names and `pub use`s):

```
#![allow(clippy::collapsible_if)]

mod api;
mod app;
mod auth;
mod autoupdate;
mod config;
mod context;
mod daily_refresh;
mod dashboard;
mod db;
mod domain;
mod error;
mod http_client;
mod image_compress;
mod image_enrichment;
mod image_store;
mod llm;
mod scheduling;
mod scrapling;
mod services;
#[cfg(test)]
mod tests;
mod types;

pub use app::{AppState, build_app};
pub use db::migrations::apply_schema_migrations;
```

- Existing image bake-off gold queries (from `docs/image-fetch-bing-html.md`). Use these exact `image_query` strings:

| id | topic_name | card_title | image_query | expected visual |
|---|---|---|---|---|
| 270 | English Grammar | Prepositions of movement | `diagram prepositions of movement from to into toward` | movement diagram (from/to/into/toward); watermarked Adobe stock is a miss |
| 286 | English Grammar | Prepositions of place | `diagram of in on at prepositions of place` | place in/on/at; time-preposition charts are a miss |
| 290 | English Grammar | Adjective order | `diagram of adjective order before nouns English grammar` | OSASCOMP / adjective-order chart |
| 45 | Rust | Clippy pedantic lints | `rust clippy pedantic lints screenshot` | Clippy lint UI |
| 8 | Helix Editor | Helix modal editor | `helix editor modal text editor screenshot` | Helix text editor, not Line 6 guitar / GitHub OG card |

- Live Bing / Playwright / DDG must **never** run in `just test` / CI. Parser fixtures already cover that (`src/llm/images/fixtures/`).
- Commands that already work: `just quick`, `just test-one <filter>`. Prefer `DENPIE_SKIP_FRONTEND_BUILD=1` on cargo.
- Do not claim real FSRS. Do not add a second image downloader. Call `crate::llm::retrieve_image` (production), never reimplement Bing parsing.

# Requirements

## 1. Library split
- Add `src/lib.rs` containing: the clippy allow, every current `mod`, `#[cfg(test)] mod tests;`, the two `pub use`s, plus `pub async fn run()` with the current server `main` body (`init_tracing`, frontend build, db, bind, serve).
- Keep existing modules **crate-private** (`mod api;`, not `pub mod api;`) except `pub mod lab;`.
- Replace `src/main.rs` with a thin binary:

```
#[tokio::main]
async fn main() {
    denpie::run().await;
}
```

- Do not change server behavior, ports, env vars, or frontend build logic. Move, do not rewrite.

## 2. Lab CLI
- `src/bin/denpie-lab.rs` only calls into the library:

```
#[tokio::main]
async fn main() {
    let code = denpie::lab::run(std::env::args().skip(1).collect()).await;
    std::process::exit(code);
}
```

- Implement `denpie::lab::run(args: Vec<String>) -> i32` in `src/lab/` (split modules as needed: `mod.rs`, `images.rs`, maybe `cases.rs`).
- Commands:
  - no args or `-h`/`--help`: print usage to stdout, exit 0
  - `list`: print benches (name, status, one-line purpose)
  - `run images` : live run (network allowed)
  - `run images --dry-run` (alias `--offline`): load cases, print the plan, write no downloads, do not call `retrieve_image`
  - `run images --strategy bing_html` (repeatable or comma-separated). Default strategy: **`bing_html` only**
  - `run images --strategy all` expands to `bing_html,bing_playwright,ddgs_text_og`
  - `run images --cases <path>` defaults to `lab/cases/images/gold.json`
  - unknown bench or unknown flag: stderr + exit 2
- `list` must include four benches:
  - `images` — status `ready`
  - `algorithms` — status `planned` (run exits 2 with "not implemented")
  - `prompts` — status `planned`
  - `cards` — status `planned`
- Do **not** start Axum, bind a port, read `DATABASE_URL`, or build the frontend.
- Init tracing in the lab binary path (`denpie=info` default) so live image logs still appear.

## 3. Image case pack + scorecard
- Commit `lab/cases/images/gold.json` with the five rows above. Include fields at least: `id`, `topic_name`, `card_title`, `card_content` (short), `image_query`, `expected` (the visual rubric).
- Live `run images`:
  - For each case × each requested strategy, call production `retrieve_image` with empty model/api_key/api_base, `ReasoningConfig::new("none")`, empty pool.
  - Record mechanical columns separately from the human visual column:
    - `search_or_download`: hit / miss (Some vs None)
    - `kind`: prepared / none (pool should not appear on this gold set)
    - `bytes`, `mime_type`, `extension` when prepared
    - `elapsed_ms`
    - `visual`: always `needs_review` (never invent a pass/fail from bytes)
  - Write artifacts under `lab/runs/<utc-timestamp>-images/` (gitignore the runs dir):
    - `scorecard.md` — table: case id, strategy, hit/miss, bytes, elapsed_ms, visual=`needs_review`, expected rubric
    - `scorecard.json` — same data
    - `cases/<id>/<strategy>.<ext>` — prepared bytes when a download succeeded
  - Print the scorecard path and a one-line summary on stdout.
  - Exit 0 even if every strategy misses (a miss is a result). Exit non-zero only on case-pack parse errors or usage errors.
- Dry-run prints the cases and strategies and does not create `lab/runs/` (stdout only is fine).

## 4. Gitignore, just, docs
- `.gitignore`: add `lab/runs/`
- `justfile`: add

```
# Opt-in research runner. Never part of just test / verify / ci.
lab *args:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo run --bin denpie-lab -- {{args}}
```

- Write `docs/lab.md` explaining: what the lab is, that it is not CI, `just lab list` / `just lab run images --dry-run` / `just lab run images`, gold set origin (`docs/image-fetch-*.md`), that `visual` is human-scored, planned benches.
- Update in the same change:
  - `README.md` Dev commands: add `just lab list` / `just lab run images --dry-run` and say it is not CI
  - `AGENTS.md` Quick start: one line pointing at `docs/lab.md`
  - `docs/feature-integration.md`: add a "Lab" home (`src/lab/`, `lab/cases/`, `just lab`)
  - `.agents/skills/add-feature/SKILL.md`: add Lab to "Also know these homes"
  - `.agents/skills/dev-loop/SKILL.md`: note `just lab` is opt-in research and must not be added to `just test` / `just verify` / `just ci`

## 5. Tests (CI-safe only)
Add focused unit tests under `src/lab/` (`#[cfg(test)]`), crate-local:
- gold JSON loads and contains the five `image_query` strings above
- `list` output (or `list_benches()`) includes `images` ready and the three planned names
- `run(["run", "images", "--dry-run"])` returns 0 and its stdout/return mentions those queries
- `run(["run", "algorithms"])` returns 2
- `run(["run", "images", "--nope"])` returns 2
- scorecard markdown helper, given fixture mechanical results, emits separate hit/miss vs `needs_review` columns (no network)

Do **not** call `retrieve_image` from tests. Do **not** hit the network. Do **not** add lab tests that need Postgres.

## Invariants you must not break
- No new protobuf / dashboard / settings / ImageStrategy variants.
- No live image fetch in `just test`.
- SM-2 remains the only production scheduler; do not add algorithm implementations in this change.
- Do not commit. Stay inside the workspace.
- rustfmt; no new deps unless you truly cannot parse argv without one (you can; do not add clap unless it is already a dependency — it is not).
- `just test` / `just ci` must not start denpie-lab live runs.

# Acceptance
Run these exactly:

```
just lab list
just lab run images --dry-run
DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace lab -- --nocapture
just quick
```

Expected:
- `just lab list` exits 0 and prints `images` (ready) plus `algorithms`, `prompts`, `cards` (planned)
- `just lab run images --dry-run` exits 0, prints the five gold queries, does not create files under `lab/runs/`
- `cargo test --workspace lab` passes with no network
- `just quick` passes (fmt check + workspace compile)

If `just lab` itself is too slow because it compiles, that is fine; it must still succeed.

# Constraints
- Do not commit unless asked.
- Stay inside the workspace.
- Use the shell and file editor only.
- Do not run a live `just lab run images` without `--dry-run` (network, slow, flaky). Wiring `retrieve_image` for the live path is required; executing it is not.
