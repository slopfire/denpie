# API v1 errors and retries

Every handled `POST /api/v1` failure returns both a non-2xx HTTP status and a
protobuf `ApiV1Response.error`. The `X-Request-Id` response header matches the
protobuf `request_id` and should be included in bug reports and server-log
searches.

## Status matrix

| HTTP | `ApiErrorCode` | Typical cause | Default `retryable` |
|---:|---|---|:---:|
| 400 | `INVALID_ARGUMENT` | Invalid protobuf, missing operation, bad field, missing/invalid idempotency key | no |
| 401 | `UNAUTHENTICATED` | Missing, invalid, or expired API key; invalid bootstrap token | no |
| 403 | `PERMISSION_DENIED` | Missing scope or forbidden key delegation | no |
| 404 | `NOT_FOUND` | Owned resource does not exist | no |
| 409 | `CONFLICT` | State conflict, capacity limit, or idempotency conflict | no, except an in-progress identical mutation |
| 415 | `UNSUPPORTED_MEDIA_TYPE` | Request is not protobuf | no |
| 422 | `INVALID_ARGUMENT` | Semantically unprocessable input | no |
| 429 | `RATE_LIMITED` | Per-source-IP rate limit | yes |
| 500-599 | `INTERNAL` | Database, filesystem, configuration, conversion, or upstream failure | yes |

The server is authoritative: use the returned `retryable` value rather than
reconstructing it from this table. Production mode redacts internal details;
the request ID is the stable diagnostic handle.

## Transport and infrastructure failures

Some failures occur before Denpie can return a protobuf body:

| Observation | Meaning | Client action |
|---|---|---|
| DNS, connection, or TLS error | The request did not receive an HTTP response | Retry reads with backoff. Retry a mutation only with its original key and exact payload. |
| Proxy-generated HTML/plain text | A reverse proxy rejected or failed the request | Record status/body/request headers; retry only if proxy policy and mutation rules allow. |
| HTTP response with truncated/invalid protobuf | Connection or intermediary corrupted the response | Treat the mutation outcome as ambiguous and retry only with the same key and payload. |
| 502/503/504 from a proxy | Upstream unavailable or timed out | Honor `Retry-After` when present and use exponential backoff with jitter. |
| Valid protobuf `INTERNAL` | Denpie handled an internal/upstream failure | Follow `retryable`; retain the request ID. |

## Idempotency-specific conflicts

- Same key and same payload: a completed result is replayed with
  `Idempotency-Replayed: true`.
- Same key and different payload: HTTP 409, not retryable. Generate a new key
  only for a genuinely new logical mutation.
- Same key still running: retryable HTTP 409 and `Retry-After: 1`. Retry the
  exact request with the same key.
- Same key left indeterminate after a process interruption: non-retryable HTTP
  409. Reconcile the affected resource before deciding on a new mutation.
- Successful API-key creation cannot replay the raw secret. Revoke any uncertain
  key and issue a new logical request with a new idempotency key.

## Recommended retry loop

Use exponential backoff with jitter and a finite attempt/deadline budget. Honor
`Retry-After` when present. Reads can be retried after transport failures;
mutations must retain the exact serialized `ApiRequest` operation and original
idempotency key across every attempt. A new key means a new logical mutation.
