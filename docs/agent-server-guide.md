# Agent Server Talk Guide

Server exposes one programmable route:

- `POST /api` -> protobuf `ApiRequest` / `ApiResponse`.

`GET /` serves browser HTML that uses `POST /api`. No JSON/session/client split remains. `/tips`, `/topics`, `/topic-classes`, `/review`, `/auth/login`, `/admin/*`, `/app/*`, and `/admin` are not API surfaces.

## Bootstrap

Use startup `admin_token` once to create full-access key:

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
- `tips`: due or generated cards.
- `review`: SRS grade or queue action.
- `get_topics` / `get_topic_classes` / `list_app_topics`: topic metadata.
- `list_tipcards` / `delete_tipcard`: card inventory.
- `get_summary`: counts.

Canonical schema: [`../proto/dailytip.proto`](../proto/dailytip.proto).
