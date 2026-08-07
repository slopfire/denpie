# Agent Server Talk Guide

## Local agent runtime

```bash
just db-up                    # PostgreSQL prerequisite
just agent-server            # isolated data, bind :3027 only, test login, smoke
just agent-server --oneshot  # start → smoke → stop
just ui-check                # frontend release build + oneshot smoke
```

Test login after bootstrap: `test` / `23452345`. Never bind or touch `:3017`.

Dashboard vision diagnostics:

| Call | Purpose |
|---|---|
| `POST /admin/settings/test-vision` | Cheap vision connectivity check (1×1 PNG, no pool write) |
| `POST /app/image-pool` response | `{ annotated, fallback_reason, model, name, … }` |

```
POST /api
ApiRequest / ApiResponse (protobuf)
Schema: proto/denpie.proto
```

| Surface | Auth | Notes |
|---|---|---|
| `POST /api` | API key | Stable client surface |
| `GET /` | session | Browser dashboard |
| `/auth/*`, `/admin/*`, `/app/*` | session | Dashboard internals |
| Legacy public routes | — | `404` |

## Bootstrap (first key)

After the first admin user exists:

```proto
ApiRequest {
  bootstrap_api_key: {
    admin_token: "token-from-log-or-settings"
    client_name: "agent"
  }
}
```

Put returned `sk_live_*` in `ApiRequest.auth` for every later call.

## Common calls

| Call | Use it for |
|---|---|
| `get_settings` / `update_settings` | LLM + runtime config |
| `create_api_key` / `list_api_keys` / `delete_api_key` | Key management |
| `tips` | Due cards, current daily cards, or cards after window rollover |
| `force_daily_refresh` | Empty = all generated topics; set `topic`/`tipcard_type` to target. Then call `tips` |
| `review` | Grade or queue action on a card |

## More calls

| Call | Use it for |
|---|---|
| `submit_custom_tipcard` | External `custom_tip` — no review row |
| `get_topics` / `list_app_topics` | Topic metadata |
| `list_tipcards` / `delete_tipcard` / `pin_tipcard` | Card inventory + pin |
| `delete_topic` | Topic + cards + reviews + images + refresh runs |
| `get_summary` | Counts |
| `add_document` / `list_documents` / `delete_document` | Grounding sources |
| `attach_document_topic` / `detach_document_topic` | Topic links without deleting the source |
| `add_pool_image` / `list_pool_images` / `delete_pool_image` | Local image pool |

Full reference: [`protobuf-api.md`](protobuf-api.md).

## Scheduling (three rules)

1. Daily refresh uses global `daily_time_zone` / `daily_update_time` unless the topic overrides count, zone, or time.
2. Pinned active cards return ahead of normal due order.
3. `max_active_cards` caps new actives; due and pinned still return.
