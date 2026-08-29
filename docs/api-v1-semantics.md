# API v1 field semantics

The protobuf schema is the type contract. This page defines the representation
rules that are easy to miss when reading field declarations alone.

## Presence and defaults

- Proto3 scalar fields that are not declared `optional` use their protobuf
  defaults: empty string, zero, or `false`. Do not infer presence from them.
- Fields declared `optional` in `UpdateSettingsRequest` and
  `UpdateTopicRequest` distinguish "leave unchanged" from "set to the scalar
  default". Use a protobuf runtime that preserves proto3 optional presence.
- Empty output strings represent unavailable optional string data. In
  particular, `ApiKeyInfo.expires_at` and `last_used_at` are empty when absent,
  and credential fields in `Settings` are empty unless the caller also has
  `secrets:read`.
- An empty response is `ApiResponse.ok`, not a missing `result` oneof.
- `EnhancePromptTemplateResult` grounding fields that are empty mean "leave
  the current setting unchanged". `prompt_template` is always filled and always
  contains `{topic}`. The operation does not persist; copy the values into
  `update_settings` or `update_topic`.

## Identifiers and numbers

- Database resource IDs are positive signed 64-bit integers. JavaScript clients
  must use a protobuf runtime configured for safe 64-bit handling; do not round
  IDs through an IEEE-754 `number` once they exceed `Number.MAX_SAFE_INTEGER`.
- Counts are non-negative. `ListFlowCardsRequest.page_size` defaults to 48 and
  is clamped to 1-100.
- `ContinueDailyReviewResponse.active_card_id`, when present, is the exact card
  made active for the requested repeatable topic. Clients should use it as the
  replacement identity for their existing topic slot. Its `pending_count` is
  the eligible unseen queue depth remaining behind that active card.
- `ReviewRequestV1.grade` accepts 0-5. Typed enum zero values are unspecified
  and rejected where the operation requires a concrete value.

## Time values

- `CreateApiKeyV1Request.expires_at` accepts an RFC 3339 timestamp with an
  explicit offset, such as `2027-01-01T00:00:00Z`. Empty means no expiration.
- Timestamp outputs are strings. New detail/page shapes serialize timestamps as
  RFC 3339, while inherited inventory/key shapes such as `TipcardInfo`,
  `DocumentInfo`, and `ApiKeyInfo` may contain PostgreSQL timestamp text (for
  example, a space instead of `T`). Parse both as offset-aware timestamps and do
  not compare their spelling lexicographically.
- `daily_time_zone` is an IANA zone such as `Asia/Vladivostok`, or a supported
  fixed-offset form. `daily_update_time` is local `HH:MM` wall time.

Future versioned messages may add a typed timestamp representation. Existing v1
string fields will keep their current meaning for wire compatibility.

## Pagination

`FlowCardPage.next_page_token` is opaque. While `has_more` is true, pass that
token back unchanged in the next `list_flow_cards` request. Do not decode,
modify, persist indefinitely, or use a token with a different user.

## Binary and image data

- `UploadDocumentRequest.data` contains raw file bytes. `mime_type` and
  `filename` describe those bytes; they do not replace server-side validation.
- `AddPoolImageRequest.image_data` and `TipsRequestV1.manual_image_data` use
  complete base64 data URLs such as `data:image/png;base64,...`.
- Individual decoded images are limited to 10 MiB. A request may carry at most
  four manual card images; the full request body limit is 56 MiB.
- `download_path` fields are same-server relative paths. Send the same Bearer
  credential when fetching them and do not treat them as permanent public URLs.
- `TipcardInfo.images` contains stored attachments for every inventory state,
  including `pending`. This lets Archive-style clients preview upcoming cards
  without promoting them into the active review flow.

## Strings and enums

- Prefer `tips_v1`, `review_v1`, and `create_api_key_v1`; their enums and
  repeated fields avoid the ambiguous comma-separated strings in legacy calls.
- Clients must tolerate enum numbers and response fields they do not recognize.
  New values and fields are additive changes under the compatibility policy.
- Topic names in `TipsRequestV1.topics` must be non-empty and cannot contain
  commas. Repeated topic IDs are set-like associations; callers should avoid
  duplicates.

## Request correlation

`ApiV1Request.request_id` is caller-selected correlation data, not a uniqueness
or idempotency mechanism. If omitted, Denpie generates a `req_*` value. The
response echoes the effective ID in both `ApiV1Response.request_id` and the
`X-Request-Id` header. Use `Idempotency-Key` separately for mutations.
