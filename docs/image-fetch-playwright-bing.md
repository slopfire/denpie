# Image fetch #2 — Playwright + Bing Images

Status: **implemented 2026-08-15** as the explicitly selectable `bing_playwright`
strategy with `scripts/bing-image-search.mjs`. This note preserves the bake-off evidence.
Siblings: [bing-html](image-fetch-bing-html.md) (implement that first), [ddgs-text-og](image-fetch-ddgs-text-og.md).

Same search engine as #1, but the SERP is loaded in headless Chromium instead of a raw HTTP GET. Ranked second: same hit rate, slower, one worse Helix false positive, better insurance if Bing stops serving `murl` in the static HTML.

## Why it exists

`#1` parses Bing’s HTML from `reqwest`. That worked on 2026-08-14. Bing is a JS-heavy SERP; a consent wall, a bot challenge, or a markup split that only appears after hydration will zero out the regex parser with no fallback.

Playwright already lives in this repo (`node_modules/playwright`, Chromium cache under `~/.cache/ms-playwright`) for e2e tests. The bake-off used that install, not a new browser.

## Bake-off evidence

Same five queries as `#1`, but the search string was lightly rewritten (e.g. `English prepositions of place in on at diagram`). Search 5/5, download 5/5, **~6.5 s/query** (about 2× `#1`).

| Card | First downloaded URL | Visual vs `#1` |
|---|---|---|
| 270 | same Adobe `ftcdn.net` movement diagram | same, watermarked |
| 286 | `7esl.com/wp-content/uploads/2018/08/1-5.jpg` | same in/on/at pyramid |
| 290 | `woodwardenglish.com/.../adjectives-word-order-english-osascomp.jpg` | cleaner OSASCOMP chart than `#1`’s first hit |
| 45 | `user-images.githubusercontent.com/766758/93236699-….png` | real Clippy diagnostic in an IDE |
| 8 | `citygame.com.ar/wp-content/uploads/helix-editor-1024x576.jpg` | **wrong product** — Line 6 Helix guitar editor |

The Helix miss is the important product note: “helix editor” is an ambiguous query. Playwright did not cause it; it just ranked a different first `murl`. `#1`’s first hit for the raw query was the text editor. Any implementation should prefer the card’s raw `image_query` and, for short brand names, keep the topic (`Helix Editor` the topic, not just `helix`).

Google Images via the same Playwright helper was **blocked or timed out** on every query. Do not add a Google path.

## How the probe worked

Node helper (`/tmp/denpie-image-eval/pw_search.mjs` — not in the repo):

```
chromium.launch({ headless: true })
page.goto(
  https://www.bing.com/images/search?q=…&form=HDRSC2,
  { waitUntil: "domcontentloaded", timeout: 25000 }
)
wait ~1500 ms
parse page.content() with the same murl regexes as #1
```

Python called it with `NODE_PATH` pointing at the repo `node_modules`. Browser: Playwright 1.62.1, `chromium-1234` already cached.

The HTML after Chromium still contained `"murl":"…"` — so Playwright is a **fetcher**, not a different parser. Share the parser with `#1`.

## Where it goes in Denpie

Follow the **Scrapling** pattern (`src/scrapling.rs`), not an in-process Chromium in the Rust binary.

1. Keep the Rust parser + `download_and_prepare` from `#1`.
2. Optional sidecar that prints a JSON URL list on stdout, exit 0 / non-zero. Env:
   - `DENPIE_BING_PLAYWRIGHT=1` (or auto if the helper + browser exist)
   - `DENPIE_DISABLE_BING_PLAYWRIGHT=1` to force HTML-only
   - `DENPIE_PLAYWRIGHT_BIN` if you do not want to hardcode `node`
3. Probe once (cached `OnceLock`, like `scrapling::status`) so missing Node/Chromium is a skip, not a job failure.
4. Call order for a combined strategy: try `#1` HTML GET first (~3 s). If zero parseable `murl`s, run Playwright. Do **not** run Playwright on every card — 6.5 s × daily refresh is a lot, and the worker loop in `src/image_enrichment.rs` sleeps 2 s only when idle.
5. `bing_playwright` is an explicit UI strategy, as requested during implementation, and does not require `search_api_key`.

Layering:

| Piece | Path |
|---|---|
| Parser + host policy | `src/llm/images/bing.rs` (shared with `#1`) |
| Optional process | `src/llm/images/bing_playwright.rs` plus `scripts/bing-image-search.mjs` |
| Dispatch | `retrieve_image` only if this is its own strategy |
| Tests | parser fixtures in Rust; sidecar argv unit test without launching Chromium |

## Implementation notes

- `waitUntil: "domcontentloaded"` plus a short extra wait was enough. `networkidle` will hang on Bing.
- Kill-on-drop and a hard timeout (30 s total) — copy Scrapling’s `kill_on_drop(true)` + `tokio::time::timeout`.
- Never pass the query through a shell. Argv only.
- Headless Chromium is large. The Docker image should **not** gain a browser unless someone explicitly opts in. Local/dev with the existing Playwright install is the intended host.
- Same UA as `#1` inside the page. Same empty `download_hosts` + `download_remote_image` for the chosen `murl`.
- If Playwright returns URLs, still run the skip-list (stock watermarks, `data:`, SVG) from the `#1` note.
- Isolation: the sidecar must not be able to hit `127.0.0.1` / RFC1918. The search URL is a constructed Bing URL (safe). The **download** is already pinned by `download_remote_image`. Do not have Playwright itself fetch the image bytes.

## Tests to add

- Sidecar argv builder (URL, timeout flags) without running Node.
- Missing binary / `DENPIE_DISABLE_*` → `None` / skip, card still saved without an image.
- Timeout → treated as no image, job can retry. Do not panic the worker.
- Shared `murl` parser tests live in `#1`; do not duplicate.
- Do **not** launch Chromium in `just test` / CI. Optional ignored test or a `#[ignore]` live check is enough.

## Legal / product caveats

Same unofficial scrape + third-party hotlink as `#1`. Playwright does not improve the license situation. It only makes the SERP fetch more like a browser.

Helix-style collisions (product name vs editor) will still happen. Topic name + card title in the query is the cheap fix; do not add an LLM ranker in the first cut.

## Do not

- Do not implement this *instead of* `#1`. HTML GET is faster and was sufficient on the sample.
- Do not add Google Images “while we have Playwright”. It was blocked.
- Do not bundle Chromium into `cargo run` or the default Docker image.
- Do not use computer-use-linux / virt-shot for this. This is a server-side fetch, not a GUI check.
- Do not touch `:3017`.

## Acceptance

When Bing’s static HTML has no `murl`s, an optional Playwright sidecar still produces a URL list that `#1`’s downloader accepts. When Node/Chromium is missing, enrichment logs a skip and the card is served without an image, same as every other failed strategy.
