# API changelog

This log records consumer-visible changes to the versioned public API. Entries
describe wire behavior rather than dashboard internals.

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
