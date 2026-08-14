# Denpie Unified Protobuf API

```
POST /api
Content-Type: application/x-protobuf
Base: http://127.0.0.1:3017
Schema: proto/denpie.proto
```

The browser UI prefers the versioned protobuf surface in [`api-v1.md`](api-v1.md)
(`POST /api/v1`) for every operation that has a v1 counterpart. After normal
session login, the same-origin cookie authorizes those calls (session principal
with full scopes); external clients still use `Authorization: Bearer sk_live_…`.
Mutations send durable idempotency keys. Remaining session JSON under `/app/*`
and `/admin/*` is only for auth/session, admin-only surfaces, or features that
still lack a v1 operation. This page documents the compatibility `POST /api`
surface. Its lifecycle is defined by the [API compatibility policy](api-compatibility.md).

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
| `get_settings` / `update_settings` | `settings` / `ok` | LLM, prompt, appearance, admin-only instance/autoupdate settings (`update` is partial) |
| `force_daily_refresh` | `force_daily_refresh` | For an explicit topic, atomically make an eligible card available; for an empty topic list, refill generated-topic queues at their eligible low-water mark |

`ForceDailyRefreshResponse.outcome` distinguishes `CARD_AVAILABLE`,
`QUEUE_REFILLED`, `NO_CHANGE`, and `ACTIVE_LIMIT_REACHED`. `available_cards`
counts cards made immediately visible and `generated_cards` counts physical rows
created. The legacy `refreshed_cards` field remains the number of topics changed.
Pending repeatable cards invalidated by later negative feedback are excluded from
both selection and low-water depth.

## Operations — inventory

| Operation | Result | Purpose |
|---|---|---|
| `create_api_key` / `list_api_keys` / `delete_api_key` | key / list / `ok` | Full-access keys; raw key never re-returned |
| `get_topics` / `list_app_topics` / `list_admin_topics` | topics | Names, due/completed counts, prompt overrides |
| `list_tipcards` / `delete_tipcard` / `pin_tipcard` | cards / `ok` | Inventory, delete, pin |
| `update_topic` / `delete_topic` | `ok` | Prompt/daily/compression/grounding/image overrides; full topic wipe |
| `get_summary` | `summary` | Card/topic counts |
| `submit_custom_tipcard` | `tips` | External `custom_tip`, no review state |

## Operations — documents & image pool

| Operation | Result | Purpose |
|---|---|---|
| `add_document` | `ok` | Document or link + zero or more `topic_ids` |
| `list_documents` | `documents` | Owned sources + `topic_ids` |
| `delete_document` | `ok` | Remove a source |
| `attach_document_topic` / `detach_document_topic` | `ok` | Add/remove one topic assignment without deleting the source |

File uploads via `POST /app/documents/upload` are converted to Markdown with [anydoc](https://crates.io/crates/anydoc) (PDF, Word, PowerPoint, Excel, OpenDocument, RTF, EPUB, CSV). HTML is stripped to text; plain text is stored as-is. Scanned/image-only PDFs are not OCR'd.
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

Generated topics behave as decks. Every grounding strategy, including factual generation, creates 5-12 cards in one batch and stores the entire batch as `pending`. `review_and_advance` applies the review and then returns the occupant of that topic slot in the same topic-locked transaction: the oldest eligible pending card if one can be promoted, otherwise an already-due active sibling. The browser commits that occupant into the existing slot in one render and does not reload the flow feed after a successful advance. A delayed overlay is only a slow-request fallback and stays on the same mounted card, in the reviewed slot's current Pins or Topic picks section. A pinned repeatable slot transfers its pin to the new occupant in the same transaction. This queue replacement bypasses `max_active_cards`, which limits newly created active cards rather than switching the reviewed card. For repeatable topics, `daily_card_count` is the number of distinct cards reviewed in that topic's daily window. An empty queue with remaining daily room keeps the outgoing card visible and polls for the refill instead of flashing a completion placeholder. After the final daily card, the browser replaces it with a persisted completion card. `Continue` starts one more full set for that topic in the current daily window.

Generated card rows and their SM-2 review rows are committed atomically, and card creation verifies that the topic belongs to the same user and card type. Batch persistence locks the topic only for the final queue-depth check and inserts, after external generation has completed; concurrent generation requests may both reach the model, but only one low-water batch is stored. Review scheduling uses the same topic-first lock order and keeps the review row locked from state load through update so concurrent submissions cannot overwrite one another. Daily eligibility and pending-card promotion share one topic-locked transaction and one bulk eligibility query across topics.

Reviewed placeholders are persisted in browser storage, so they survive a page reload until the real card becomes due again or is deleted.

Automatic illustrations use durable `card_image_jobs`. Generated cards enqueue
their image work in the same transaction as card creation. A lease-based worker
resolves pool/API/web sources outside database transactions, retries failures,
and marks completion only after the prepared bytes and `tipcard_images` metadata
are stored. Promotion never waits for an external image provider. A worker that
stops after attachment but before acknowledging the job detects the existing
attachment on retry and completes without creating a duplicate.

Restricted web-image sources keep search policy separate from download policy:
`search_domains` constrains provider discovery, while `download_hosts` is the
exact allowlist for returned image bytes. Older source JSON without
`search_domains` uses `download_hosts` for both. Source search instructions are
included in the provider query.


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
| `POST /api/v1` | Bearer API key | Recommended versioned protobuf surface |
| `POST /api` | API key | Compatibility protobuf surface |
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
