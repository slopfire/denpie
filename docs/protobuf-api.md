# Denpie Unified Protobuf API

```
POST /api
Content-Type: application/x-protobuf
Base: http://127.0.0.1:3017
Schema: proto/denpie.proto
```

Browser at `GET /` uses the same endpoint with a session cookie. API clients use an API key.

## Auth

Set `ApiRequest.auth` to raw `sk_live_*` on every call except `bootstrap_api_key`. Server stores SHA-256 only.

### First key

```proto
ApiRequest {
  bootstrap_api_key: {
    admin_token: "token-from-settings-yaml"
    client_name: "desktop"
  }
}
```

## Operations — core

| Operation | Result | Purpose |
|---|---|---|
| `bootstrap_api_key` | `api_key_created` | First key from `admin_token` |
| `tips` | `tips` | Due cards, current daily cards, new cards after refresh, or `manual_tip` |
| `review` | `ok` | Grade or queue action |
| `get_settings` / `update_settings` | `settings` / `ok` | LLM, prompt, theme, appearance, autoupdate (`update` is partial) |
| `force_daily_refresh` | `force_daily_refresh` | Refill selected generated-topic queues that are empty or have 1-2 pending cards; does not reschedule current cards |

## Operations — inventory

| Operation | Result | Purpose |
|---|---|---|
| `create_api_key` / `list_api_keys` / `delete_api_key` | key / list / `ok` | Full-access keys; raw key never re-returned |
| `get_topics` / `list_app_topics` / `list_admin_topics` | topics | Names, due/completed counts, prompt overrides |
| `list_tipcards` / `delete_tipcard` / `pin_tipcard` | cards / `ok` | Inventory, delete, pin |
| `update_topic` / `delete_topic` | `ok` | Prompt/daily/compression/icon overrides; full topic wipe |
| `get_summary` | `summary` | Card/topic counts |
| `submit_custom_tipcard` | `tips` | External `custom_tip`, no review state |

## Operations — documents & image pool

| Operation | Result | Purpose |
|---|---|---|
| `add_document` | `ok` | Document or link + zero or more `topic_ids` |
| `list_documents` | `documents` | Owned sources + `topic_ids` |
| `delete_document` | `ok` | Remove a source |
| `attach_document_topic` / `detach_document_topic` | `ok` | Add/remove one topic assignment without deleting the source |
| `add_pool_image` / `list_pool_images` / `delete_pool_image` | ok / `pool_images` / `ok` | Local image pool |

`get_settings` / `update_settings` expose `search_provider` (`tavily` or `firecrawl`) and
`scrape_provider` (`scrapling`, `firecrawl`, or `direct`) alongside `search_api_key` and
`search_base_url`.

Link documents are scraped according to `scrape_provider` (default **`scrapling`**):

| `scrape_provider` | Behavior |
|---|---|
| `scrapling` | Main local option. Runs the Scrapling CLI (`scrapling extract get … --ai-targeted`) when installed; falls back to capped direct HTTP if the CLI is missing. |
| `firecrawl` | Cloud scrape via `/v2/scrape` (pages and supported remote files such as PDFs). Requires `search_api_key`. |
| `direct` | Legacy capped HTTP GET with HTML tags stripped. |

Install Scrapling with `pip install "scrapling[fetchers,shell]"`. Override the binary with
`DENPIE_SCRAPLING_BIN`; disable with `DENPIE_DISABLE_SCRAPLING=1`.

## Daily retrieval (`tips`)

### Repeatable learning

Repeatable cards use three learning actions:

| Action | Effect |
|---|---|
| `again` | Keep the card active, schedule an earlier SM-2 review, and signal that simpler context may help |
| `learned` | Keep the card active, schedule an easy SM-2 review, and signal that the next generated concept can be slightly more advanced |
| `skip_known` | Dismiss the card and treat its content as known vocabulary |
| `skip_not_interested` | Dismiss the card and steer future generation away from similar subject matter or framing |
| `skip_too_difficult` | Dismiss the card and request an easier prerequisite or example |

Generation receives recent titles and compact card content grouped by feedback. Unseen repeatable backlog cards older than the latest feedback are skipped, so stale generation cannot override the learner signal. The model infers a useful next concept; this is personalization around the existing SM-2 scheduler, not a separate scheduling algorithm.

Generated topics behave as decks. Every grounding strategy, including factual generation, creates 5-10 cards in one batch and stores the entire batch as `pending`. A card request promotes and returns the oldest pending card immediately; an available card never waits for an LLM refill. This queue replacement bypasses `max_active_cards`, which limits newly created active cards rather than switching the reviewed card. Page loading promotes one pending card when a repeatable topic has no due card, and an explicit topic Load rotates to an existing pending card first, so pending-only decks survive reloads. An empty queue is bootstrapped on demand, while refresh paths replenish queues observed at the low-water mark of one or two pending cards. Repeatable topics expose one stable browser card per topic: the active replacement changes its content without remounting the slot or closing fullscreen. A reviewed completion placeholder appears only when no replacement is available; its `Learn more` button refills that topic and refreshes the flow.

Reviewed placeholders are persisted in browser storage, so they survive a page reload until the real card becomes due again or is deleted.


Legacy `repeat`, `memorize`, and `dismiss` actions remain accepted as aliases for API clients already using them.

For each requested scheduled topic/type, returns in order:

1. Due active cards
2. Cards already created in the current daily window (up to `daily_card_count`)
3. The oldest promoted pending card after an on-demand low-water refill

Window: `settings.daily_time_zone` (IANA or `UTC±HH`) + `settings.daily_update_time` (`HH:MM`, default `00:00`). Override per topic via `update_topic`.

Bad zone/time → `UTC`, midnight, one card.

### Manual / custom cards

| Type | How | Notes |
|---|---|---|
| `manual_tip` | `TipsQuery.tipcard_type = "manual_tip"` + `manual_content` | Optional `manual_compressed_content`. No LLM. |
| `custom_tip` | `submit_custom_tipcard` | No `review_states` row. Still in lists and counts. |

## Compression

`settings.compression_level`: `light` · `balanced` · `strong` · `ultra`. Invalid → `balanced`.

Topic override: `update_topic.compression_level` (empty = inherit). Fenced code blocks stay; prose is compacted.

## Grounding

| Setting | Role |
|---|---|
| `Settings.grounding_model` | Model for non-factual grounding + agentic web image search |
| `Settings.grounding_reasoning_effort` | Reasoning override for that model |

Empty → inherit `model` / `reasoning_effort`. Both are optional partial fields on `update_settings`.

The "From My Data" grounding strategy (protocol id `rag`) only uses sources assigned to the current topic. Documents/links are reusable via `topic_ids`. Unassigned sources are not retrieved.

**Empty title on pasted docs:** compression model titles from the first 4,000 chars (cap 32 tokens). Model down → first ten words. Links still need a title.

**Dashboard URL explore:** one URL → read TOC (including mdBook `toc.html`) → up to 100 unique same-origin pages → replace the textbox with those URLs → add as individual sources.

## Active card limit

`Settings.max_active_cards` caps `active` review states. `0` = unlimited.

At cap:

- `tips` still returns due/pinned
- does not create new generated cards
- manual create → `409 Conflict`

## Pinning

Pinned active cards sit in a top section and return ahead of schedule even when not due. Reviews still update SM-2. Unpin → normal due-date order.

```proto
ApiRequest {
  auth: "sk_live_..."
  pin_tipcard: { id: 123 pinned: true }
}
```

## HTTP surfaces

| Surface | Auth | Notes |
|---|---|---|
| `POST /api` | API key or session | Stable public surface |
| `GET /` | session | Dashboard |
| `/auth/*`, `/admin/*`, `/app/*` | session | Dashboard internals, not the key API |
| Legacy public routes | — | `404` |

## Status codes

| Case | Status |
|---|---:|
| Success | `200` |
| Bad protobuf / missing operation | `400` |
| Bad `admin_token` or API key | `401` |
| Missing card/topic for mutation | `404` |
| Active-card cap on manual create | `409` |
| SQL / settings / stored-state failure | `500` |
