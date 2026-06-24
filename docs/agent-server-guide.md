# Agent Server Talk Guide

One API-key route: `POST /api` with protobuf `ApiRequest` / `ApiResponse`.  
Schema: [`proto/denpie.proto`](../proto/denpie.proto).

`GET /` is the browser dashboard. `/auth/*`, `/admin/*`, `/app/*` are session-authenticated dashboard routes. Legacy routes (`/tips`, `/topics`, `/topic-classes`, `/review`, `/auth/login`, `/admin`) return `404`.

## Bootstrap

After the first admin user exists, use the startup `admin_token` to create an admin-owned key:

```proto
ApiRequest {
  bootstrap_api_key: {
    admin_token: "token-from-log-or-settings"
    client_name: "agent"
  }
}
```

Use the returned `sk_live_*` in `ApiRequest.auth` for all later calls.

## Common Operations

- `get_settings` / `update_settings`: LLM and runtime config.
- `create_api_key` / `list_api_keys` / `delete_api_key`: key management.
- `tips`: due cards, current daily topic cards, or generated cards after the refresh window rolls over.
- `force_daily_refresh`: empty fields refresh all generated topics; `topic`/`tipcard_type` target selected topics. Then call `tips` for the cards.
- `submit_custom_tipcard`: external card stored as `custom_tip` with no review row.
- `review`: grade or queue action on a card.
- `get_topics` / `list_app_topics`: topic metadata.
- `list_tipcards` / `delete_tipcard`: card inventory.
- `delete_topic`: delete topic + cards + review state + images + daily refresh runs.
- `get_summary`: counts.

## Scheduling Notes

- Daily refresh uses global `daily_time_zone` / `daily_update_time` unless a topic overrides `daily_card_count`, `daily_time_zone`, or `daily_update_time`.
- Pinned active cards are returned ahead of normal due-date order.
- `max_active_cards` caps new active cards; due and pinned cards are still returned.
