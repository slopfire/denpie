# Image fetch #1 — Bing Images HTML scrape

Status: **implemented 2026-08-15** as the `bing_html` strategy in
`src/llm/images/bing.rs`. This note preserves the bake-off evidence and design rationale.
Siblings: [playwright-bing](image-fetch-playwright-bing.md), [ddgs-text-og](image-fetch-ddgs-text-og.md).

This was the bake-off winner. Implement this one first.

## Why it exists

Admin’s live path is `image_strategy = web_search` + `search_provider = tavily` with an **empty** `search_api_key`. `src/llm/images/web_search.rs` returns `None` immediately when the key is empty, so enrichment jobs 270 / 286 / 290 all failed with `image strategy returned no usable image`.

Danbooru and Safebooru are enabled as `kind: api` sources, but those only run under `ImageStrategy::Programmatic`. They are the wrong corpus for English-grammar diagrams anyway (Danbooru 422 on English tag soup; Safebooru empty body).

Bing Images HTML search needs **no API key** and, on 2026-08-14 from this host, returned pedagogically correct diagrams for the three failed cards.

## Bake-off evidence

Queries were the cards’ stored `image_query` values, plus two extra topic checks.

| Card | Query | First downloaded URL | Visual |
|---|---|---|---|
| 270 | `diagram prepositions of movement from to into toward` | `as2.ftcdn.net/.../1000_F_354214329_….jpg` | Correct movement diagram (Adobe stock watermark) |
| 286 | `diagram of in on at prepositions of place` | `i.pinimg.com/originals/36/89/d1/….jpg` | Correct in/on/at pyramid (also covers time) |
| 290 | `diagram of adjective order before nouns English grammar` | `sdo.nsuem.ru/.../Adjectives-order-in-English-grammar.png` | Correct OSASCOMP-style chart |
| 45 | `rust clippy pedantic lints screenshot` | `user-images.githubusercontent.com/69764315/165403908-….png` | Clippy lint UI (not specifically pedantic) |
| 8 | `helix editor modal text editor screenshot` | `cdn-images-1.readmedium.com/.../1*fo-ttva….png` | Real Helix tutorial, not the Line 6 guitar product |

Mechanical: search 5/5, download 5/5, ~2.8 s/query. Visual quality was the best of every method tried (Commons book covers, Wikipedia lead images, Met Museum art, Openverse army Flickr, current Tavily/Firecrawl/Danbooru all failed).

Other Bing candidate hosts that appeared and looked usable: `englishan.com`, `7esl.com`, `www.woodwardenglish.com`, `test-english.com`, `static.vecteezy.com`, `screens.cdn.wordwall.net`.

## How the probe worked

```
GET https://www.bing.com/images/search?q={urlencoded query}&form=HDRSC2&cc=us&setlang=en
User-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36
Accept-Language: en-US,en;q=0.9
```

`cc=us` / `setlang=en` are a consent-wall mitigation. If the HTML still looks like
a cookie/consent page and has no `murl` / `mediaurl` metadata, discovery fails
closed and the enrichment job completes without an image.

Parse the HTML with **all** of these (Bing emits more than one encoding of the same field):

1. `"murl":"(https?://[^"]+)"` then JSON/unicode unescape
2. `murl&quot;:&quot;(https?://[^&]+?)&quot;` then URL-unescape
3. `mediaurl=(https?%3A%2F%2F[^&]+)` then URL-unescape

Keep unique `http(s)` URLs, try them in order, stop at the first file whose magic bytes are PNG / JPEG / WebP / GIF. The existing `image_store::download_remote_image` already does SSRF, redirect, size, and magic-byte checks — **reuse it**. Do not add a second downloader.

A desktop Chrome UA was required. A library default UA is likely to get a consent wall or empty markup.

## Where it goes in Denpie

Do **not** overload Tavily/Firecrawl. This is a different provider with no key.

Implemented shape:

1. `src/llm/images/bing.rs` owns static discovery and the checked-in parser fixture.
2. `ImageStrategy::BingHtml` uses setting string `bing_html` and dispatches in `src/llm/images/mod.rs`.
3. Global and per-topic settings expose the strategy with copy in `frontend/src/i18n/en.json`.
4. Domain: `ImageStrategy::from_setting` / `as_str` in `src/domain/grounding.rs`. Unknown values must keep falling back to `None`.
5. Enrichment still calls `retrieve_image` from `src/services/image_enrichment.rs`; no new job table was needed. A strategy miss completes the job without an image. Saving a new image strategy requeues failed jobs and completed jobs that never attached an image. Legacy remote strategy values resolve to `bing_html`.

Tavily/Firecrawl remain fact-grounding providers and no longer own automatic image retrieval.

Do **not** put this behind `ImageSourceKind::WebSearch` + non-empty `download_hosts`. Agentic isolated search (`src/llm/images/agentic.rs`) **requires** a host allowlist; Bing result hosts are unbounded (Pinterest, 7ESL, GitHub, Adobe, random ESL blogs). Empty `download_hosts` + `download_remote_image` (public DNS pin, no private IPs) is the correct safety model — same as today’s `web_search::retrieve`, which already calls `retrieve_with_policy(..., &[], &[], "")`.

## Implementation notes

- Send a browser UA on the **search** GET. The download path can keep the hardened client; some CDNs also want a `Referer` of the image’s own origin (the probe set that and it helped).
- Cap candidates (5 is enough; the first hit was correct on all five queries).
- Skip `data:` URLs, SVG placeholders, and obvious tracking pixels.
- Optional skip-list for watermarked stock CDNs (`ftcdn.net`, `*.alamy.com`, `*.shutterstock.com`, `*.istockphoto.com`). Card 270’s first hit was a watermarked Adobe image; later Bing URLs for that query (`englishan.com`, `static.vecteezy.com`) were cleaner. Prefer skipping the first host over “no image”.
- Do not require `search_api_key`. Do require a non-empty `image_query` (already on the card).
- Timeouts: search HTML ~10 s, each download already bounded by `download_remote_image`. The enrichment worker retries transport failures 3 times; empty search results are not retried.
- Persist nothing but the compressed bytes via `replace_card_prepared_image` — same as every other strategy.
- Markup will rot. Unit-test the parser against a **checked-in HTML fixture**, not live Bing. Live Bing does not belong in `just test` / CI.

## Tests to add

- Parser: fixture with `"murl":"https://…"` and the `&quot;` form → same URL list, order preserved, dups dropped.
- Parser: consent/empty HTML → empty list, no panic.
- Strategy: empty `image_query` → `None`.
- Strategy: first URL fails download, second is a real PNG → `Some(Prepared)`. Mock or temp file; do not hit Bing.
- `ImageStrategy::from_setting("bing_html")` round-trips. Unknown string still `None`.
- Existing `just test-one image` / enrichment tests must stay green. Do not change SM-2 or claim FSRS.

## Legal / product caveats

This is an unofficial scrape of Bing’s image SERP, then a hotlink download of third-party files. Results are ESL blogs, Pinterest, stock CDNs, GitHub user uploads. Fine for a personal instance; do not describe this as “licensed” or Commons-clean. If license matters more than pedagogy, this is the wrong method (Commons lost the bake-off on relevance).

## Do not

- Do not call this FSRS or change scheduling.
- Do not bind or restart `:3017`. Agent work stays on `:3027`.
- Do not use Danbooru/Safebooru as a fallback for these grammar queries.
- Do not add Firecrawl here. `FIRECRAWL_API_KEY` exists in the environment but `api.firecrawl.dev` timed out on IPv4 and IPv6 from this host.
- Do not follow Bing’s own thumbnail/CDN hosts (`th.bing.com`, `tse*.mm.bing.net`) if you can get `murl` — those are transcodes, not the source diagram.

## Acceptance

A local run with admin’s three failed queries downloads a PNG/JPEG/WebP that a human would accept on the card, without a Tavily key, through `download_remote_image`, and failed jobs can be requeued by changing `image_strategy` and saving settings.
