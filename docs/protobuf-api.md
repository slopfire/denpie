# Daily Tip Unified Protobuf API

Base URL for local development:

```text
http://127.0.0.1:3001
```

`POST /api` is the canonical API for both card clients and administration. The server is single-user, so an API key has full access to tips, reviews, settings, API-key management, topic metadata, card deletion, and summary counts.

Requests and responses use `application/x-protobuf` with the canonical schema in [`proto/dailytip.proto`](../proto/dailytip.proto).

The root page at `GET /` is a browser control panel that uses this same protobuf endpoint.

## Authentication

Every `ApiRequest` has an `auth` field. Set it to the raw `sk_live_*` API key for every operation except `bootstrap_api_key`.

To create the first key without a session cookie, call `bootstrap_api_key` with the startup `admin_token`:

```proto
ApiRequest {
  bootstrap_api_key: {
    admin_token: "token-from-settings-yaml"
    client_name: "desktop"
  }
}
```

The response contains `api_key_created.api_key`. Store it client-side; the server stores only a SHA-256 hash.

## Operations

`ApiRequest.op` is a `oneof`:

| Operation | Result | Purpose |
|---|---|---|
| `bootstrap_api_key` | `api_key_created` | Create the first/full-access API key using `admin_token`. |
| `tips` | `tips` | Get due cards, reuse the current daily topic card, generate a new card after the configured daily update time, or create a manual card from user text. |
| `review` | `ok` | Review, dismiss, acknowledge, repeat, or memorize a card. |
| `get_topics` | `topics` | List known topic names. |
| `get_topic_classes` | `topic_classes` | List topic classes and card behavior types. |
| `get_settings` | `settings` | Read LLM, prompt, theme, and autoupdate settings. |
| `update_settings` | `ok` | Update provided settings fields. Unset optional fields are preserved. |
| `create_api_key` | `api_key_created` | Create another full-access API key. |
| `list_api_keys` | `api_keys` | List key metadata. Raw keys are never returned. |
| `delete_api_key` | `ok` | Delete a key by database ID. |
| `list_admin_topics` | `admin_topics` | List topics with prompt overrides. |
| `list_tipcards` | `tipcards` | List stored cards with status and repeat count. |
| `delete_tipcard` | `ok` | Delete a card and its review state. |
| `delete_topic` | `ok` | Delete a topic and all of its cards and review states. |
| `get_summary` | `summary` | Read card/topic counts. |
| `list_app_topics` | `app_topics` | Read topic rows with due/completed counts. |
| `update_topic` | `ok` | Set or clear a topic prompt override. |

## Daily Topic Retrieval

`tips` is topic-aware. For each requested SRS topic/type, the server first returns due active cards. If none are due, it returns existing cards created in the current daily window up to that topic's daily card count. New cards are generated only until that per-topic daily count is satisfied.

Daily windows use `settings.daily_time_zone` (IANA name such as `UTC`, `Asia/Vladivostok`, or `America/New_York`; fixed offsets such as `UTC+10` are also accepted) and `settings.daily_update_time` (`HH:MM`, default `00:00`). Each topic can override count/time with `update_topic.daily_card_count`, `update_topic.daily_time_zone`, and `update_topic.daily_update_time`. Invalid values fall back to `UTC`, midnight, and one card.

For user-authored cards, set `TipsQuery.tipcard_type` to `manual_tip` and provide `manual_content`. The server stores that text directly as the full card content, uses `manual_compressed_content` when provided, and otherwise uses the full text as compact content. Manual cards do not call the LLM.

## Removed Routes

Legacy route-specific APIs are gone. `/tips`, `/topics`, `/topic-classes`, `/review`, `/auth/login`, `/admin/*`, and `/app/*` return `404 Not Found`. Use `POST /api` for all client and admin operations. `GET /` is only the HTML control page.

## Status Codes

| Case | Status |
|---|---:|
| Success | `200 OK` |
| Invalid protobuf body or missing operation | `400 Bad Request` |
| Invalid `admin_token` for bootstrap | `401 Unauthorized` |
| Missing or invalid API key | `401 Unauthorized` |
| Missing card/topic for mutation | `404 Not Found` |
| SQL, settings, or stored-state failure | `500 Internal Server Error` |
