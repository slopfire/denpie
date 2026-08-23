# Agent Server Talk Guide

## Local agent runtime

```bash
just db-up                    # PostgreSQL prerequisite
just agent-server            # isolated data, bind :3027 only, test login, smoke
just agent-server --oneshot  # start → smoke → stop
just ui-check                # Astro build + oneshot smoke
just frontend-astro-runtime  # alias of ui-check
just playwright              # Astro Playwright on :3027
just agent-server --frontend-dist frontend-astro/dist  # serve a pre-built Astro dist
```

Test login after bootstrap: `test` / `23452345`. Never bind or touch `:3017`.

Dashboard vision diagnostics:

| Call | Purpose |
|---|---|
| `POST /admin/settings/test-vision` | Cheap vision connectivity check (1×1 PNG, no pool write). Sends `reasoning.effort=none` and a 1024-token cap so thinking models such as MiniMax-M3 still return message content. |
| `POST /app/image-pool` response | `{ annotated, fallback_reason, model, name, … }` |

```
POST /api
ApiRequest / ApiResponse (protobuf)
Schema: proto/denpie.proto
```

For new automation use `POST /api/v1`, `ApiV1Request` / `ApiV1Response`, and
`Authorization: Bearer ...`. The browser SPA also uses `POST /api/v1` after
session login (cookie principal). See [`api-v1.md`](api-v1.md).

| Surface | Auth | Notes |
|---|---|---|
| `POST /api/v1` | Bearer API key **or** browser session | Recommended; request IDs + structured errors + durable mutation idempotency |
| `GET /api/v1/*-images/*` | Bearer **or** session | Card/pool image bytes for clients and browser `<img>` |
| `POST /api` | API key | Compatibility surface |
| `GET /` | session | Browser dashboard shell |
| `/auth/*` | session | Login, logout, passkeys, profile |
| `/admin/*`, `/app/*` | session | Remaining dashboard-only ops without a v1 counterpart |
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
| `tips` | Due cards, current daily cards, or cards after window rollover; repeatable topics stop at their per-topic daily limit |
| `force_daily_refresh` | Empty topics refill all generated queues. An explicit topic atomically makes one eligible card available and reports a structured outcome; use `available_cards` instead of inferring availability from `refreshed_cards`. |
| dashboard `POST /app/continue-daily-review` | Browser-only continuation for one repeatable topic: adds another full `daily_card_count` set to its current daily window and makes its next card available |
| dashboard `POST /app/topics/suggest-icons` / `set-icon` | AI icon picker: `suggest-icons` returns 5 allowlisted icon ids for a topic (`{ id, excluded_icons? }` → `{ icons: [...] }`); exclusions apply only to that request. `set-icon` applies one (`{ id, icon_id }` → `{ icon_id }`) |
| `review` | Grade or queue action on a card |

## More calls

| Call | Use it for |
|---|---|
| `submit_custom_tipcard` | External `custom_tip` — no review row |
| `get_topics` / `list_app_topics` | Topic metadata |
| `list_tipcards` / `delete_tipcard` / `pin_tipcard` | Card inventory + pin |
| `append_tipcard_images` / `replace_tipcard_images` | Card image append, replace, and clear |
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
