# Denpie API v1

API v1 is the recommended integration surface. It keeps Denpie's protobuf
contract while adding standard Bearer authentication, request correlation,
structured errors, scoped keys, pagination, resource details, binary downloads,
typed card/review requests, and durable idempotency for mutations.

## Documentation set

| Need | Reference |
|---|---|
| Every operation, message, result, scope, and mutation policy | [Generated operation reference](api-v1-reference.md) |
| Presence, timestamps, IDs, pagination, binary fields | [Field semantics](api-v1-semantics.md) |
| HTTP/protobuf errors, infrastructure failures, retries | [Errors and retries](api-v1-errors.md) |
| Versioning and deprecation guarantees | [Compatibility policy](api-compatibility.md) |
| Rules and gates for API contributors | [API development rules](api-development-rules.md) |
| Consumer-visible changes | [API changelog](api-changelog.md) |
| Executable curl, Python, TypeScript, and Rust clients | [API examples](../examples/api/README.md) |
| Canonical wire schema | [`proto/denpie.proto`](../proto/denpie.proto) |
| Versioned descriptor/source bundle | [Schema bundle](../api/schema/v1/README.md) |

## Endpoints

| Endpoint | Purpose |
|---|---|
| `POST /api/v1` | Versioned protobuf request/response envelope |
| `GET /api/v1/tipcard-images/{id}` | Authenticated card image bytes (Bearer or session) |
| `GET /api/v1/pool-images/{id}` | Authenticated pool image bytes (Bearer or session) |
| `POST /api` | Compatibility endpoint for existing `ApiRequest` clients |

`POST /api/v1` accepts `application/x-protobuf` and `application/protobuf`.
Send API credentials in `Authorization: Bearer sk_live_...`, or put the raw key
in `ApiRequest.auth`. The browser SPA instead relies on the logged-in session
cookie: when no Bearer/body key is present, a valid session is accepted as a
full-access principal so same-origin UI (and `<img src>` image GETs) work after
normal login without storing a raw key.

## Envelope

```proto
ApiV1Request {
  request_id: "sync-019fe4"
  call: {
    get_api_info: {}
  }
}
```

Successful calls return HTTP 200 and `ApiV1Response.success`. Handled failures
use the appropriate non-2xx HTTP status and `ApiV1Response.error`:

```proto
ApiV1Response {
  request_id: "sync-019fe4"
  error: {
    code: API_ERROR_CODE_PERMISSION_DENIED
    message: "API key requires scope 'settings:write'"
    retryable: false
  }
}
```

Omit `request_id` to let the server generate a `req_*` identifier. Supplied IDs
must be 1-64 ASCII letters, digits, `.`, `_`, or `-`.

## Idempotent mutations

Every mutating API v1 operation requires an idempotency key. Send it in the
conventional `Idempotency-Key` HTTP header or as `ApiV1Request.idempotency_key`. If
both are supplied, they must match. Keys are 1-128 ASCII letters, digits, `.`,
`_`, `:`, or `-`; a UUID is a good default.

```proto
ApiV1Request {
  request_id: "create-document-attempt-2"
  idempotency_key: "3df7b78d-613b-45d4-a057-77f11c607acd"
  call {
    create_document {
      source_type: "document"
      title: "API design notes"
      content: "Retries should not create duplicates."
    }
  }
}
```

The key is scoped to the authenticated credential and exact protobuf payload:

- Repeating the same key and payload within 24 hours returns the stored status
  and response without executing the mutation again. The new `request_id` is
  retained for correlation.
- A replay includes `Idempotency-Replayed: true`; every idempotent response
  echoes `Idempotency-Key`.
- Reusing a key with a different payload returns HTTP 409.
- Concurrent identical requests wait up to one second for the first result,
  then return a retryable HTTP 409 with `Retry-After: 1` if it is still running.
- If a process stops after a side effect but before recording its response, the
  in-progress key remains locked. After five minutes the 409 becomes
  non-retryable and reports an indeterminate outcome. Denpie never executes that
  key again automatically, favoring duplicate prevention over guessing.
- Completed records expire after 24 hours and are cleaned up opportunistically.

Successful API-key creation is at-most-once but not replayable: Denpie never
stores the raw generated credential in the idempotency table. A retry returns
HTTP 409 instructing the caller to revoke the uncertain key and use a new
idempotency key. Validation errors from key creation remain replayable.

Read-only operations do not require a key. Supplying one is harmless but does
not cache the response. The compatibility endpoint `POST /api` retains its
original non-idempotent behavior.

## Discovery

`get_api_info` is intentionally unauthenticated. It returns the API version,
server/build versions, and feature capabilities. Clients should use it instead
of guessing support from the application version.

```bash
printf '%s\n' 'request_id: "docs-discovery"' 'call { get_api_info {} }' |
protoc --proto_path=proto --encode=denpie.ApiV1Request proto/denpie.proto |
curl --silent --show-error \
  -H 'Content-Type: application/x-protobuf' \
  --data-binary @- http://127.0.0.1:3017/api/v1 |
protoc --proto_path=proto --decode=denpie.ApiV1Response proto/denpie.proto
```

For authenticated reads and safe mutations, continue with the
[tested multi-language examples](../examples/api/README.md). CI compiles or
executes every example and checks that their schema and operation reference are
current.

## Scoped keys

Keys created by the dashboard or legacy `create_api_key` remain full-access for
compatibility. New integrations should use `create_api_key_v1` and grant only
the scopes they need.

| Scope | Allows |
|---|---|
| `cards:read` | Summary, card inventory/detail/feed, card image downloads |
| `cards:write` | Generate/create/delete/pin cards, refresh and continuation |
| `reviews:write` | Submit reviews |
| `topics:read` / `topics:write` | Read or mutate topics |
| `settings:read` / `settings:write` | Read or change settings |
| `secrets:read` | Include raw LLM/search credentials in `get_settings` |
| `keys:manage` | Create, list, or revoke keys |
| `documents:read` / `documents:write` | Read or mutate grounding sources |
| `images:read` / `images:write` | Read or mutate the local image pool |
| `diagnostics:run` | Run vision connectivity diagnostics |
| `*` | Full access; intended for trusted administration only |

`expires_at` is optional RFC 3339. Expired keys authenticate exactly like an
invalid key. Key listings include scopes, expiry, and last-use time. A key with
`settings:read` but without `secrets:read` receives empty credential fields.
Keys may only delegate scopes they already hold, and a delegated key may not
outlive the key that created it. Only a non-expiring full-access key can use the
legacy operation that creates another unrestricted key.

```proto
ApiV1Request {
  idempotency_key: "create-desktop-widget-key-2026-08-09"
  call {
    create_api_key_v1 {
      client_name: "desktop-widget"
      scopes: "cards:read"
      scopes: "reviews:write"
      expires_at: "2027-01-01T00:00:00Z"
    }
  }
}
```

## Preferred typed operations

- `tips_v1` uses repeated topic names and `TipcardTypeValue`.
- `review_v1` uses `ReviewActionValue` and rejects grades outside 0-5.
- `list_flow_cards` uses a server-issued opaque `page_token`; `page_size`
  defaults to 48 and is clamped to 1-100.
- `get_tipcard` returns title, visual metadata, content, state, and downloadable
  image metadata.
- `create_document` and `upload_document` return the created document and ID.
- `create_pool_image` returns the ID, annotation diagnostics, tags, and download
  path.
- `get_document`, `explore_link`, `continue_daily_review`, and
  `test_vision_model` close the previous browser-only gaps.

Treat page tokens as opaque and pass `next_page_token` back unchanged while
`has_more` is true.

## Compatibility endpoint

`POST /api` continues to accept the original unwrapped `ApiRequest`, body-based
authentication, string-valued card/review fields, and legacy result shapes.
It also accepts Bearer authentication and the additive operations, but errors
remain plain-text for compatibility. New clients should use `/api/v1`.

## Limits and retry behavior

- Versioned requests are limited to 56 MiB so a manual card can carry up to four
  validated image data URLs. Individual decoded images remain limited to 10 MiB.
- API v1 is rate-limited per source IP at 10 requests/second with a burst of 50.
- Retry only when `ApiError.retryable` is true. When retrying a mutation, reuse
  its original idempotency key and payload; never generate a new key for an
  ambiguous attempt.

See [API v1 errors and retries](api-v1-errors.md) for the complete HTTP/error
matrix, reverse-proxy and transport failures, and the recommended retry loop.
