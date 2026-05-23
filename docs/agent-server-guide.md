# Agent Server Talk Guide

Server exposes one API-key programmable route:

- `POST /api` -> protobuf `ApiRequest` / `ApiResponse`.

`GET /` serves browser HTML. `/auth/*`, `/admin/*`, and `/app/*` are session-authenticated dashboard routes, not stable API-key surfaces. `/tips`, `/topics`, `/topic-classes`, `/review`, `/auth/login`, and `/admin` return `404`.

## Bootstrap

After setup creates first admin user, use startup `admin_token` to create admin-owned key:

```proto
ApiRequest {
  bootstrap_api_key: {
    admin_token: "token-from-log-or-settings"
    client_name: "agent"
  }
}
```

Then put returned `sk_live_*` into `ApiRequest.auth` for every request.

## Common Ops

- `get_settings` / `update_settings`: LLM and runtime config.
- `create_api_key` / `list_api_keys` / `delete_api_key`: key management.
- `tips`: due cards, current daily topic cards, or generated cards after topic refresh window rolls over.
- `force_daily_refresh`: empty fields refresh all generated topics; topic/type fields target one set. Then call `tips` for fresh cards.
- `submit_custom_tipcard`: external card. Stored as grey `custom` / `custom_tip`. No review row.
- Daily card refresh uses global `daily_time_zone` / `daily_update_time`, unless topic overrides `daily_card_count`, `daily_time_zone`, or `daily_update_time`.
- `review`: schedule grade or queue action.
- `get_topics` / `list_app_topics`: topic metadata.
- `list_tipcards` / `delete_tipcard`: card inventory.
- `delete_topic`: delete topic + cards + review state + card images + daily refresh runs.
- `get_summary`: counts.

Canonical schema: [`../proto/denpie.proto`](../proto/denpie.proto).
