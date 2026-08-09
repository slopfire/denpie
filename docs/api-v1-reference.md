# API v1 operation reference

This file is generated from [`api/operations-v1.json`](../api/operations-v1.json)
and checked against `ApiRequest.op` and `ApiResponse.result` in
[`proto/denpie.proto`](../proto/denpie.proto). Do not edit it by hand; run
`just api-reference` after changing the manifest or protobuf operation set.

The API currently exposes **40 operations**: **15 reads** and
**25 mutations**. Every mutation requires an idempotency key. See the
[API v1 guide](api-v1.md) for transport behavior and the
[field semantics](api-v1-semantics.md) for representation details.

| Operation | Request message | Success result | Authentication | Required scope | Kind | Idempotency | Purpose |
|---|---|---|---|---|---|---|---|
| `bootstrap_api_key` | `BootstrapApiKeyRequest` | `ApiKeyCreated` | admin token in request | `none` | one-time secret mutation | required; success is not replayable | Create the first API key after the first admin user exists. |
| `tips` | `TipsQuery` | `TipsResponse` | Bearer API key | `cards:write` | mutation | required; replayable | Legacy string-valued card retrieval and generation; prefer tips_v1. |
| `review` | `ReviewPayload` | `Empty` | Bearer API key | `reviews:write` | mutation | required; replayable | Legacy string-valued review submission; prefer review_v1. |
| `get_topics` | `Empty` | `GetTopicsResponse` | Bearer API key | `topics:read` | read | not required | List topic names owned by the API-key user. |
| `get_settings` | `Empty` | `Settings` | Bearer API key | `settings:read` | read | not required | Read user settings; credential fields additionally require secrets:read. |
| `update_settings` | `UpdateSettingsRequest` | `Empty` | Bearer API key | `settings:write` | mutation | required; replayable | Partially update user settings using proto3 optional fields. |
| `create_api_key` | `CreateApiKeyRequest` | `ApiKeyCreated` | Bearer API key | `keys:manage` | one-time secret mutation | required; success is not replayable | Create an unrestricted key; only a non-expiring full-access key may call it. |
| `list_api_keys` | `Empty` | `ApiKeys` | Bearer API key | `keys:manage` | read | not required | List key metadata without returning raw key values. |
| `delete_api_key` | `DeleteByIdRequest` | `Empty` | Bearer API key | `keys:manage` | mutation | required; replayable | Revoke an API key owned by the current user. |
| `list_admin_topics` | `Empty` | `AdminTopics` | Bearer API key | `topics:read` | read | not required | List topic configuration for administration clients. |
| `list_tipcards` | `Empty` | `Tipcards` | Bearer API key | `cards:read` | read | not required | List the user's complete tipcard inventory. |
| `delete_tipcard` | `DeleteByIdRequest` | `Empty` | Bearer API key | `cards:write` | mutation | required; replayable | Delete a tipcard owned by the current user. |
| `get_summary` | `Empty` | `AppSummary` | Bearer API key | `cards:read` | read | not required | Return topic and card counts for the current user. |
| `list_app_topics` | `Empty` | `AppTopics` | Bearer API key | `topics:read` | read | not required | List topics with card counts and effective configuration. |
| `update_topic` | `UpdateTopicRequest` | `Empty` | Bearer API key | `topics:write` | mutation | required; replayable | Partially update one topic's overrides. |
| `delete_topic` | `DeleteByIdRequest` | `Empty` | Bearer API key | `topics:write` | mutation | required; replayable | Delete a topic and its owned topic data. |
| `pin_tipcard` | `PinTipcardRequest` | `Empty` | Bearer API key | `cards:write` | mutation | required; replayable | Set or clear a tipcard's pinned state. |
| `submit_custom_tipcard` | `CustomTipcardRequest` | `TipsResponse` | Bearer API key | `cards:write` | mutation | required; replayable | Create an external custom card without review state. |
| `force_daily_refresh` | `ForceDailyRefreshRequest` | `ForceDailyRefreshResponse` | Bearer API key | `cards:write` | mutation | required; replayable | Refill selected generated-topic queues at their low-water mark. |
| `add_document` | `AddDocumentRequest` | `Empty` | Bearer API key | `documents:write` | mutation | required; replayable | Legacy document creation without returning the new ID; prefer create_document. |
| `list_documents` | `Empty` | `Documents` | Bearer API key | `documents:read` | read | not required | List grounding-source metadata. |
| `delete_document` | `DeleteByIdRequest` | `Empty` | Bearer API key | `documents:write` | mutation | required; replayable | Delete a grounding source. |
| `add_pool_image` | `AddPoolImageRequest` | `Empty` | Bearer API key | `images:write` | mutation | required; replayable | Legacy pool-image creation without diagnostics; prefer create_pool_image. |
| `list_pool_images` | `Empty` | `PoolImages` | Bearer API key | `images:read` | read | not required | List local image-pool metadata. |
| `delete_pool_image` | `DeleteByIdRequest` | `Empty` | Bearer API key | `images:write` | mutation | required; replayable | Delete an image from the local pool. |
| `attach_document_topic` | `AttachDocumentTopicRequest` | `Empty` | Bearer API key | `documents:write` | mutation | required; replayable | Assign an existing document to one topic. |
| `detach_document_topic` | `AttachDocumentTopicRequest` | `Empty` | Bearer API key | `documents:write` | mutation | required; replayable | Remove one topic assignment from an existing document. |
| `get_api_info` | `Empty` | `ApiInfo` | none | `none` | read | not required | Discover the API, server/build versions, and capabilities. |
| `list_flow_cards` | `ListFlowCardsRequest` | `FlowCardPage` | Bearer API key | `cards:read` | read | not required | Page through dashboard-ready cards using opaque cursor tokens. |
| `get_tipcard` | `GetByIdRequest` | `TipcardDetail` | Bearer API key | `cards:read` | read | not required | Get one card with content, state, visuals, and image metadata. |
| `get_document` | `GetByIdRequest` | `DocumentDetail` | Bearer API key | `documents:read` | read | not required | Get one grounding source including its stored content. |
| `continue_daily_review` | `ContinueDailyReviewRequest` | `ContinueDailyReviewResponse` | Bearer API key | `cards:write` | mutation | required; replayable | Start another daily review set for selected topics. |
| `explore_link` | `ExploreLinkRequest` | `ExploredLinks` | Bearer API key | `documents:write` | read | not required | Discover same-origin links without storing documents. |
| `test_vision_model` | `Empty` | `VisionModelTest` | Bearer API key | `diagnostics:run` | read | not required | Test the configured vision-model connection. |
| `create_document` | `AddDocumentRequest` | `DocumentDetail` | Bearer API key | `documents:write` | mutation | required; replayable | Create a text or link source and return the stored document and ID. |
| `upload_document` | `UploadDocumentRequest` | `DocumentDetail` | Bearer API key | `documents:write` | mutation | required; replayable | Upload a supported file, convert it to Markdown, and return the document. |
| `create_pool_image` | `AddPoolImageRequest` | `PoolImageCreated` | Bearer API key | `images:write` | mutation | required; replayable | Create and annotate a pool image and return its ID and diagnostics. |
| `tips_v1` | `TipsRequestV1` | `TipsResponse` | Bearer API key | `cards:write` | mutation | required; replayable | Retrieve or generate cards using repeated topics and typed card kinds. |
| `review_v1` | `ReviewRequestV1` | `Empty` | Bearer API key | `reviews:write` | mutation | required; replayable | Submit a grade and typed review action for one card. |
| `create_api_key_v1` | `CreateApiKeyV1Request` | `ApiKeyCreated` | Bearer API key | `keys:manage` | one-time secret mutation | required; success is not replayable | Create a least-privilege scoped key with optional expiration. |

## Interpreting the table

- `scope: none` is limited to discovery and first-key bootstrap. Every other
  operation authenticates the key first, then authorizes the listed scope.
- A full-access `*` key satisfies every scope. `secrets:read` is an additional
  disclosure gate for credential fields returned by `get_settings`.
- A replayable mutation returns the stored HTTP status and protobuf response
  for the same credential, idempotency key, and exact operation payload.
- Successful key creation is deliberately not replayable because Denpie never
  persists raw generated credentials in the idempotency store.
- `explore_link` and `test_vision_model` are classified as reads for transport
  idempotency even though they may perform external I/O.
