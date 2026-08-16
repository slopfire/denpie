# Image fetch #3 — ddgs text search + `og:image`

Status: **implemented 2026-08-15** as the explicitly selectable `ddgs_text_og`
strategy with `scripts/ddgs-image-search.py` and Rust Open Graph extraction.
Siblings: [bing-html](image-fetch-bing-html.md) (implement first), [playwright-bing](image-fetch-playwright-bing.md).

Third place. Use as a **no-key fallback** when Bing HTML (and Playwright) return nothing — not as the default. It is the only method that found *something* downloadable for all five queries without a paid key, but quality is mixed and it is an order of magnitude slower.

## Why it exists

Bing (`#1` / `#2`) can vanish (block, consent, empty `murl`). Direct DuckDuckGo image endpoints were unreliable from this host:

- `ddgs.images()` : 2/5 (timeouts on three queries), but the two hits were excellent ESL diagrams
- raw `duckduckgo.com/i.js` after a `vqd` token: **5/5 connect timeout**
- Qwant / SearXNG / Brave / Startpage / Yandex / Google HTML: 403, 429, or empty

The working DDG-shaped path was: **web text search** → take page URLs → pull `og:image` (or a direct `*.png`/`*.jpg` href). The machine already has the Python package `ddgs` 9.14.4 (`Dux Distributed Global Search`).

## Bake-off evidence

Query = card `image_query` plus ` diagram filetype:png OR filetype:jpg`. Search 5/5, download 5/5, **~34 s/query**.

| Card | What came back | Visual |
|---|---|---|
| 270 | ESL worksheet (`og:image` of up/through/down/into/from…to) | Useful, cartoon, not as clean as Bing’s diagram |
| 286 | Time-preposition fill-in email (`in` / `on` / `at` used as **time**) | **Wrong sense** — card is place, not time |
| 290 | Downloaded image (Quizizz / similar) | Partial; first `og:image` is a page preview, not a dedicated chart |
| 45 | First candidate was a `data:image/svg+xml;base64,…` placeholder; second was a Twitter avatar | **Miss** — must reject `data:` and tiny profile JPEGs |
| 8 | `opengraph.githubassets.com/…/samwhelp/helix-editor` | GitHub Open Graph card, not a screenshot of the editor |

So: reliable *bytes*, unreliable *illustration*. Bing won because the first `murl` *was* the diagram. This method often illustrates the *landing page*.

`ddgs.images()` on the rewritten query later hit 3/5 with good 7ESL/Pinterest diagrams when DDG did not time out. If you implement a DDG path, prefer `images()` and only fall back to text+og.

## How the probe worked

```python
from ddgs import DDGS

with DDGS() as ddgs:
    rows = list(ddgs.text(f"{query} diagram filetype:png OR filetype:jpg", max_results=8))

# For each href:
#   if href looks like an image → keep
#   else GET the HTML (10 s) and take the first of:
#     <meta property="og:image" content="…">
#     <meta content="…" property="og:image">
#     <link rel="image_src" href="…">
#   resolve relative URLs with urljoin
```

Then the same magic-byte download as `#1`.

`ddgs` aggregates several backends; it is not an official DuckDuckGo API. Treat it as an unofficial metasearch, same legal class as Bing HTML.

## Where it goes in Denpie

Do **not** add `ddgs` as a Rust crate dependency and do **not** make it the default `image_strategy`.

Preferred: optional sidecar, same spirit as Scrapling / the Playwright note.

1. Helper script (Python) that reads a query on argv and prints a JSON list of image URLs. Use `ddgs` if importable, else exit 2 (missing).
2. Env: `DENPIE_DDGS_BIN`, `DENPIE_DISABLE_DDGS=1`. Probe once.
3. Rust module `src/llm/images/ddgs.rs` that:
   - runs the sidecar **or** implements only the `og:image` extraction in Rust (small, testable) after a URL list
   - feeds each URL to `download_and_prepare` with an **empty** host allowlist
4. Wire as the explicit `ddgs_text_og` strategy. The dead Danbooru/Safebooru/Tavily image modes were removed rather than adding another option beside them.

If you refuse a Python sidecar: porting `ddgs` into Rust is a project. The `i.js` contract is undocumented and timed out here. Do not spend the first implementation week on that.

`og:image` extraction itself **should** be Rust. It is HTML regex/DOM, unit-testable, and useful for any future “search returned a page, not an image” path (including Bing if `murl` is missing but a page URL remains).

## Implementation notes

Hard filters — the bake-off failed without them:

- Reject `data:` URLs (card 45).
- Reject SVG, especially tiny 320×320 gradient placeholders.
- Reject Open Graph defaults: `opengraph.githubassets.com`, Twitter/X `pbs.twimg.com/profile_images/`, favicon-sized files, anything under ~20 KB unless it is a real PNG diagram.
- Reject `og:image` that is the site logo (URL contains `logo`, `icon`, `avatar`, `sprite`).
- Prefer URLs whose path contains `diagram`, `preposition`, `clippy`, `screenshot`, or a file extension `.png`/`.jpg`/`.webp`.
- Cap **pages fetched** (3) and **total wall time** (12–15 s). 34 s is too slow for `process_one`. The worker should fail the attempt and retry, not hold a lease that long.
- Append `diagram` only when the query does not already contain it. Do not blindly add `filetype:png` if you also want WebP.
- Use the card’s raw `image_query`. The place-vs-time miss on card 286 came from a generic “prepositions in/on/at” page. Topic name (`English grammar`) in the text query would have helped more than `filetype:`.

SSRF: every `og:image` is untrusted. `download_remote_image` only. Do not have the sidecar download bytes.

## Tests to add

- `og:image` extractor: fixture HTML for each of the three tag shapes; relative URL resolved; missing tag → `None`.
- Reject `data:image/svg+xml;base64,…`.
- Reject githubassets / profile_images hosts (unit list).
- Sidecar missing → skip, no worker panic.
- Timeout / non-zero exit → `None`.
- Do not call DuckDuckGo or `ddgs` from CI.

## Legal / product caveats

Unofficial metasearch + whatever `og:image` the page advertises. Often a social preview, not a reusable diagram. Worse license posture than Commons, worse pedagogy than Bing. The only reason it is on this list is: **no key, 5/5 downloads, works when image SERPs flake.**

## Do not

- Do not replace `#1` with this.
- Do not enable this by default on admin. Their three jobs need Bing-quality diagrams, not GitHub OG cards.
- Do not treat `ddgs.images()` timeouts as “DDG is dead” — retry once, then fall through.
- Do not add Jina (`s.jina.ai` was 401), Unsplash napi (401), Pexels/Pixabay (need keys), or Firecrawl (unreachable from this host) as part of this work.
- Do not touch `:3017`.

## Acceptance

Given a query and no Tavily/Bing result, the path returns at most a few public image URLs extracted from real pages, rejects placeholders, downloads through `download_remote_image`, and gives up in well under the enrichment lease time. A human looking at card 270 might accept the result; cards 45 and 8 from the bake-off should have been rejected by the filters above — if your implementation still attaches a Twitter avatar, the filters are wrong.
