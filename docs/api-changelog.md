# API changelog

This log records consumer-visible changes to the versioned public API. Entries
describe wire behavior rather than dashboard internals.

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
