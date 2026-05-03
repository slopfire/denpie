# Denpie Unified Protobuf API

Base URL for local development:

```text
http://127.0.0.1:3017
```

`POST /api` is the canonical API for both card clients and administration. The server is single-user, so an API key has full access to tips, reviews, settings, API-key management, topic metadata, card deletion, card pinning, and summary counts.

Requests and responses use `application/x-protobuf` with the canonical schema in [`proto/denpie.proto`](../proto/denpie.proto).

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
| `tips` | `tips` | Get due cards, reuse the current daily topic card, generate a new card after the configured daily card refresh time, or create a manual card from user text. |
| `submit_custom_tipcard` | `tips` | Store an externally supplied custom card without creating scheduling review state. |
| `review` | `ok` | Review, dismiss, acknowledge, repeat, or memorize a card. |
| `get_topics` | `topics` | List known topic names. |
| `get_topic_classes` | `topic_classes` | List topic classes and card behavior types. |
| `get_settings` | `settings` | Read LLM, prompt, theme, appearance, and server self-update settings. |
| `update_settings` | `ok` | Update provided settings fields. Unset optional fields are preserved. |
| `create_api_key` | `api_key_created` | Create another full-access API key. |
| `list_api_keys` | `api_keys` | List key metadata. Raw keys are never returned. |
| `delete_api_key` | `ok` | Delete a key by database ID. |
| `list_admin_topics` | `admin_topics` | List topics with prompt overrides. |
| `list_tipcards` | `tipcards` | List stored cards with status, repeat count, pin state, and next scheduled review time. |
| `delete_tipcard` | `ok` | Delete a card and its review state. |
| `pin_tipcard` | `ok` | Pin or unpin a card by database ID. Pinned active cards are treated as due until unpinned. |
| `force_daily_refresh` | `force_daily_refresh` | Move current unpinned generated cards out of the active daily view for all generated topics or selected topics so the next `tips` call creates fresh daily cards. |
| `delete_topic` | `ok` | Delete a topic and all of its cards and review states. |
| `get_summary` | `summary` | Read card/topic counts. |
| `list_app_topics` | `app_topics` | Read topic rows with due/completed counts. |
| `update_topic` | `ok` | Set or clear a topic prompt override. |

## Daily Topic Retrieval

`tips` is topic-aware. For each requested SRS topic/type, the server first returns due active cards. If none are due, it returns existing cards created in the current daily refresh window up to that topic's daily card count. New cards are generated only until that per-topic daily count is satisfied.

Daily card refresh windows use `settings.daily_time_zone` (IANA name such as `UTC`, `Asia/Vladivostok`, or `America/New_York`; fixed offsets such as `UTC+10` are also accepted) and `settings.daily_update_time` (`HH:MM`, default `00:00`). Each topic can override count/time with `update_topic.daily_card_count`, `update_topic.daily_time_zone`, and `update_topic.daily_update_time`. Invalid values fall back to `UTC`, midnight, and one card.

Use `force_daily_refresh` with empty fields to refresh all existing generated topics, or with comma-separated topics plus the desired `topic_class` and `tipcard_type` to target selected topics before the normal refresh time. The operation schedules active, unpinned generated cards forward and returns `refreshed_cards`; pinned cards stay in place, and refreshed cards are not marked dismissed.

For user-authored cards, set `TipsQuery.tipcard_type` to `manual_tip` and provide `manual_content`. The server stores that text directly as the full card content, uses `manual_compressed_content` when provided, and otherwise uses the full text as compact content. Manual cards do not call the LLM.

## Custom Tipcards

Use `submit_custom_tipcard` for cards that come from external workflows such as email summaries, reminders, or non-client automations. The server stores the card as `tipcard_type = "custom_tip"` under the `topic_class = "custom"` class and returns those names in the `tips.tips[0]` response. Custom cards do not create a `review_states` row, so scheduling algorithms never schedule or update them. They still appear in card lists and total card counts.

```proto
ApiRequest {
  auth: "sk_live_..."
  submit_custom_tipcard: {
    topic: "email summary"
    full_content: "Ship digest at 09:00."
    compressed_content: "Digest 09:00"
    title: "Morning digest"
  }
}
```

The browser dashboard marks `custom_tip` cards with a grey class stripe.

## Active Card Limit

Set `Settings.max_active_cards` with `update_settings.max_active_cards` to cap cards whose review state is `active`. `0` means unlimited. When the cap is reached, `tips` still returns existing due or pinned cards, but it does not create new generated cards; manual card creation returns `409 Conflict`.

## Pinning Cards

Pinning is a scheduling override for active cards. A pinned card remains visible in the control panel's separate top section and is returned ahead of normal scheduled cards even when `next_review_at` is in the future. Reviews still update the card's SRS state and next scheduled review time; unpinning restores normal due-date behavior.

```proto
ApiRequest {
  auth: "sk_live_..."
  pin_tipcard: {
    id: 123
    pinned: true
  }
}
```

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
