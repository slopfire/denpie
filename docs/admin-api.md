# Daily Tip Admin API

Base URL for local development:

```text
http://127.0.0.1:3001
```

The admin API uses JSON and cookie sessions. Log in with the `admin_token` from `settings.yaml` or from the startup console output. After successful login, reuse the returned session cookie for all `/admin/*` API routes.

The public HTML dashboard is available at:

```http
GET /admin
```

## Session Authentication

### `POST /auth/login`

Creates an admin session.

Request:

```json
{
  "admin_token": "token-from-settings-yaml"
}
```

Response body: empty

Status codes:

| Case | Status |
|---|---:|
| Login success | `200 OK` |
| Wrong or empty token | `401 Unauthorized` |
| Session write failure | `500 Internal Server Error` |

Example:

```bash
curl -i \
  -c cookies.txt \
  -H 'Content-Type: application/json' \
  -d '{"admin_token":"token-from-settings-yaml"}' \
  http://127.0.0.1:3001/auth/login
```

All routes below require the session cookie:

```bash
curl -b cookies.txt http://127.0.0.1:3001/admin/settings
```

Unauthenticated admin API requests return `401 Unauthorized`.

## Settings

Settings are persisted in `settings.yaml`.

### `GET /admin/settings`

Returns current LLM configuration.

Response:

```json
{
  "model": "google/gemini-3.1-flash",
  "template": "Give a smart tip about {topic}.",
  "api_key": "",
  "base_url": "https://openrouter.ai/api/v1"
}
```

Defaults are used when keys are missing from `settings.yaml`.

### `POST /admin/settings`

Replaces the LLM configuration fields in `settings.yaml`. Existing unrelated keys, such as `admin_token`, are preserved.

Request:

```json
{
  "model": "google/gemini-2.5-pro",
  "template": "Give a smart tip about {topic}.",
  "api_key": "provider-api-key",
  "base_url": "https://openrouter.ai/api/v1"
}
```

Response:

```json
null
```

Status: `200 OK` on success.

## API Key Management

Client API keys authenticate `/tips`, `/topics`, and `/review`.

### `POST /admin/keys`

Creates a new client API key. The raw key is returned once and cannot be recovered later.

Request:

```json
{
  "client_name": "desktop_widget"
}
```

`client_name` is optional. If omitted, the server uses `default_client`.

Response:

```json
"sk_live_generated_key_value"
```

Status: `200 OK` on success.

### `GET /admin/keys`

Lists stored API key metadata. Raw keys are not returned.

Response:

```json
[
  {
    "id": 1,
    "client_name": "desktop_widget",
    "created_at": "2026-04-25 10:30:00"
  }
]
```

Status: `200 OK` on success.

### `DELETE /admin/keys`

Deletes a client API key by database ID.

Request:

```json
{
  "id": 1
}
```

Response body: empty

Status codes:

| Case | Status |
|---|---:|
| Delete SQL executed | `200 OK` |
| SQL failure | `500 Internal Server Error` |

Note: deleting a missing ID still returns `200 OK` because the SQL statement succeeds with zero affected rows.

## Knowledge Base Views

These admin routes return JSON snapshots used by the dashboard.

### `GET /admin/topics`

Response:

```json
[
  {
    "id": 1,
    "name": "rust"
  }
]
```

Status: `200 OK` on success.

### `GET /admin/tipcards`

Response:

```json
[
  {
    "id": 1,
    "topic_name": "rust",
    "full_content": "Full tip text",
    "compressed_content": "Short tip text",
    "created_at": "2026-04-25 10:30:00"
  }
]
```

Status: `200 OK` on success.

## Admin Flow Example

```bash
BASE_URL=http://127.0.0.1:3001
ADMIN_TOKEN=token-from-settings-yaml

curl -c cookies.txt \
  -H 'Content-Type: application/json' \
  -d "{\"admin_token\":\"$ADMIN_TOKEN\"}" \
  "$BASE_URL/auth/login"

curl -b cookies.txt "$BASE_URL/admin/settings"

curl -b cookies.txt \
  -H 'Content-Type: application/json' \
  -d '{"client_name":"telegram_bot"}' \
  "$BASE_URL/admin/keys"
```

