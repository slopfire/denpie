# Denpie Unified Protobuf API

Base URL: `http://127.0.0.1:3017`  
All programmable calls: `POST /api`  
Content-Type: `application/x-protobuf`  
Schema: [`proto/denpie.proto`](../proto/denpie.proto)

The browser dashboard at `GET /` uses the same endpoint but authenticates via session cookie. API clients authenticate with an API key.

## Authentication

Set `ApiRequest.auth` to the raw `sk_live_*` API key for every operation except `bootstrap_api_key`. The server stores only a SHA-256 hash of the key.

To create the first admin-owned key, use the startup `admin_token`:

```proto
ApiRequest {
  bootstrap_api_key: {
    admin_token: "token-from-settings-yaml"
    client_name: "desktop"
  }
}
```

## Operations

| Operation | Result | Purpose |
|---|---|---|
| `bootstrap_api_key` | `api_key_created` | First key from `admin_token`. |
| `tips` | `tips` | Due cards, current daily topic cards, newly generated cards after refresh window, or a user-authored `manual_tip` card. |
| `submit_custom_tipcard` | `tips` | Store an external card (`custom_tip`) with no review state. |
| `review` | `ok` | Grade or queue action on a card. |
| `get_topics` | `topics` | Known topic names. |
| `get_settings` | `settings` | LLM, prompt, theme, appearance, autoupdate. |
| `update_settings` | `ok` | Partial update; unset optional fields are preserved. |
| `create_api_key` | `api_key_created` | Create another full-access key. |
| `list_api_keys` | `api_keys` | Key metadata; raw keys never returned. |
| `delete_api_key` | `ok` | Delete a key by database ID. |
| `list_admin_topics` | `admin_topics` | Topics with prompt overrides. |
| `list_tipcards` | `tipcards` | Cards with status, repeat count, pin state, next review time. |
| `delete_tipcard` | `ok` | Delete a card and its review state. |
| `pin_tipcard` | `ok` | Pin or unpin a card by ID. |
| `force_daily_refresh` | `force_daily_refresh` | Generate fresh cards for all or selected generated topics without rescheduling current cards. |
| `delete_topic` | `ok` | Delete a topic plus its cards, review states, images, and refresh runs. |
| `get_summary` | `summary` | Card/topic counts. |
| `list_app_topics` | `app_topics` | Topics with due/completed counts. |
| `update_topic` | `ok` | Set or clear a topic prompt override, daily overrides, compression level, and icon. |

## Daily Retrieval

`tips` is topic-aware. For each requested scheduled topic/type it returns:

1. Due active cards.
2. Existing cards created in the current daily refresh window (up to the topic's `daily_card_count`).
3. Newly generated cards only until that per-topic count is reached.

The daily window is defined by `settings.daily_time_zone` (IANA or `UTC±HH`) and `settings.daily_update_time` (`HH:MM`, default `00:00`). A topic can override count, time zone, and update time via `update_topic`.

Invalid time zone/time fall back to `UTC`, midnight, and one card.

### Manual and custom cards

- `manual_tip`: set `TipsQuery.tipcard_type = "manual_tip"` and provide `manual_content`. Optional `manual_compressed_content`. No LLM call.
- `custom_tip`: use `submit_custom_tipcard`. Stored as `custom_tip` with no `review_states` row; appears in lists and counts.

## Compression

`settings.compression_level` chooses the preset: `light`, `balanced`, `strong`, `ultra`. Invalid values fall back to `balanced`. A topic can override via `update_topic.compression_level` (empty string inherits global). Fenced code blocks are preserved; surrounding prose is compacted.

## Active Card Limit

`Settings.max_active_cards` caps cards whose review state is `active`. `0` means unlimited. When the cap is reached, `tips` still returns due or pinned cards but does not create new generated cards; manual creation returns `409 Conflict`.

## Pinning

Pinned active cards stay in a top section and are returned ahead of normal scheduled cards even when not yet due. Reviews still update scheduling state; unpinning restores normal due-date behavior.

```proto
ApiRequest {
  auth: "sk_live_..."
  pin_tipcard: { id: 123 pinned: true }
}
```

## HTTP Surfaces

Legacy public routes (`/tips`, `/topics`, `/topic-classes`, `/review`, `/auth/login`, `/admin`) return `404`. `GET /` serves the dashboard; `/auth/*`, `/admin/*`, `/app/*` are dashboard implementation details, not the stable API-key surface.

## Status Codes

| Case | Status |
|---|---:|
| Success | `200 OK` |
| Invalid protobuf or missing operation | `400 Bad Request` |
| Invalid `admin_token` | `401 Unauthorized` |
| Missing or invalid API key | `401 Unauthorized` |
| Missing card/topic for mutation | `404 Not Found` |
| SQL, settings, or stored-state failure | `500 Internal Server Error` |
