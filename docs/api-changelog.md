# API changelog

This log records consumer-visible changes to the versioned public API. Entries
describe wire behavior rather than dashboard internals.

## 2026-08-24 - Per-topic grounding model overrides

- `AdminTopic` and `AppTopicInfo` now expose optional per-topic grounding model
  and reasoning-effort overrides. `UpdateTopicRequest` can set either field;
  an empty value clears the override and inherits the user setting.

## 2026-08-23 — Versioned tipcard image editing

- Added replayable `append_tipcard_images` and `replace_tipcard_images`
  operations. Clients can attach validated uploads, owned pool images, or safe
  remote images, and can replace or clear an owned card's image list without
  using dashboard-only JSON endpoints.

## 2026-08-16 — Stable Continue slot identity

- `ContinueDailyReviewResponse.active_card_id` additively identifies the exact
  repeatable card prepared for the requested topic, while `pending_count`
  carries its remaining queue depth. Clients can replace the existing topic
  slot directly instead of reloading and re-sorting the flow.

## 2026-08-15 — Pending-card image inventory

- `TipcardInfo.images` additively exposes stored image metadata and authenticated
  download paths for every inventory state, including `pending`. Inventory clients
  can preview those images without promoting or reviewing the card.

## 2026-08-15 — Correct rate-limit replenishment intervals

- Fixed the `POST /api/v1` and authenticated image limiters to replenish at
  their documented 10 requests/second and 50 requests/second rates. The
  `tower-governor` builder value is a per-token interval, so the previous
  configuration replenished only once every 10 and 50 seconds respectively.

## 2026-08-14 — Explicit daily-refresh outcomes

- `ForceDailyRefreshResponse` additively exposes `outcome`, `available_cards`,
  and `generated_cards`; `refreshed_cards` remains the legacy count of topics
  changed.
- A targeted `force_daily_refresh` now promotes one eligible pending or newly
  generated card in the same topic-locked transaction. Pending cards invalidated
  by later negative feedback no longer block low-water generation.

## 2026-08-14 — Repeatable slot occupant

- `review_and_advance` now returns the occupant of the reviewed topic slot:
  a newly promoted pending card when one exists, otherwise an already-due
  active sibling in the same topic. The browser commits that card into the
  existing slot without a full feed reload. A completion placeholder is only
  written when the daily set is finished or no refill is coming.

## 2026-08-14 — Structured rate-limit errors

- `POST /api/v1` 429s now return a protobuf `ApiV1Response.error` with
  `RATE_LIMITED` instead of tower-governor's plain-text `"Too Many Requests"`
  body. Browser clients were decoding that text as protobuf (`unexpected end
  group tag`) and immediately retrying the mutation.
- Authenticated image downloads no longer share the `POST /api/v1` burst of 50.
  They use 50 requests/second with a burst of 200.

## 2026-08-09 — API v1 initial contract

- Added `POST /api/v1` with `ApiV1Request`/`ApiV1Response` envelopes,
  correlation IDs, structured protobuf errors, and conventional Bearer auth.
- Added scoped, expiring API keys and least-privilege delegation through
  `create_api_key_v1`.
- Added durable 24-hour idempotency for every v1 mutation, including replay,
  payload-conflict detection, concurrent-call coordination, and fail-closed
  handling of ambiguous writes.
- Added cursor-paginated flow cards, card/document detail, document upload,
  image-pool creation diagnostics, authenticated image downloads, link
  exploration, daily-review continuation, and vision-model diagnostics.
- Added typed `tips_v1` and `review_v1` operations. Legacy string-valued
  operations remain available.
- Added `review_and_advance`, which atomically records a review and returns the
  next eligible flow card under one durable idempotency key.
- Added `sources` to `FlowCardInfo` (and a new `CardSource` message): each flow
  card now carries the grounding documents/links assigned to its topic, so
  clients can render card sources without extra per-card lookups.
- Published a generated complete operation reference, field/error semantics,
  compatibility policy, schema packaging, and tested curl, Python, TypeScript,
  and Rust examples.
- Added an additive-only v1 contract ledger, exhaustive build-generated
  operation/result mapping, runtime success-variant validation, and mandatory
  local/CI contract gates.

## Legacy compatibility surface

`POST /api` predates versioned changelog tracking. It remains supported as
described in [the compatibility reference](protobuf-api.md), without v1's
structured-error and durable-idempotency guarantees.
