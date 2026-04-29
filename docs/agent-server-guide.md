# Agent Server Talk Guide

Use this when agent need talk with Daily Tip Server. Caveman doc mode, but exact protocol stay exact.

## Server Facts

- Local server: `http://127.0.0.1:3001`
- Start: `cargo run`
- Compile check: `cargo check`
- DB: `dailytip.db`
- Schema auto-run from `schema.sql` on startup.
- Admin token live in `settings.yaml` as `admin_token`. If missing, server create and print token on startup.
- Client API key created through admin API/dashboard. Raw key returned once.

## Route Map

- `GET /admin` -> public HTML dashboard. No session needed.
- `POST /auth/login` -> JSON login. Creates cookie session.
- `GET /admin/settings` -> JSON, session required.
- `POST /admin/settings` -> JSON, session required.
- `GET /admin/keys` -> JSON, session required.
- `POST /admin/keys` -> JSON, session required.
- `DELETE /admin/keys` -> JSON, session required.
- `GET /admin/topics` -> JSON, session required.
- `GET /admin/tipcards` -> JSON, session required.
- `POST /tips` -> protobuf, client API key required.
- `GET /topics` -> protobuf, client API key required.
- `GET /topic-classes` -> protobuf, client API key required.
- `POST /review` -> protobuf, client API key required.

## Auth Rules

Admin routes:

- Login first with `POST /auth/login`.
- Send JSON: `{"admin_token":"..."}`.
- Store and reuse cookie.
- Missing/wrong session -> `401`.

Client routes:

- Send raw API key in `Authorization`.
- No `Bearer`.
- Good: `Authorization: sk_live_abc123`
- Bad: `Authorization: Bearer sk_live_abc123`
- Missing/wrong key -> `401`.

## Protobuf Rules

Schema source: `proto/dailytip.proto`.

Messages:

```proto
message TipsQuery {
  uint64 count = 1;
  string topics = 2;
  string topic_class = 3;
  string tipcard_type = 4;
}

message ReviewPayload {
  int64 card_id = 1;
  uint32 grade = 2;
  string action = 3;
}

message TipsResponse {
  repeated TipCardResponse tips = 1;
}

message GetTopicsResponse {
  repeated string topics = 1;
}

message GetTopicClassesResponse {
  repeated TopicClass classes = 1;
}
```

Headers for protobuf routes:

```http
Authorization: sk_live_your_client_key
Content-Type: application/x-protobuf
Accept: application/x-protobuf
```

## Exact Agent Workflow

1. Check server running:

```bash
curl -i http://127.0.0.1:3001/admin
```

Expect `200 OK` and HTML. If connection fail, start server with `cargo run`.

2. Get admin token:

```bash
python - <<'PY'
import yaml
with open("settings.yaml") as f:
    print(yaml.safe_load(f).get("admin_token", ""))
PY
```

If empty/missing, run server once. It will create token.

3. Login and save cookie:

```bash
BASE_URL=http://127.0.0.1:3001
ADMIN_TOKEN=token-from-settings-yaml
curl -i -c /tmp/dailytip.cookies \
  -H 'Content-Type: application/json' \
  -d "{\"admin_token\":\"$ADMIN_TOKEN\"}" \
  "$BASE_URL/auth/login"
```

Expect `200 OK`.

4. Read settings:

```bash
curl -s -b /tmp/dailytip.cookies "$BASE_URL/admin/settings"
```

5. Create client API key:

```bash
API_KEY=$(curl -s -b /tmp/dailytip.cookies \
  -H 'Content-Type: application/json' \
  -d '{"client_name":"agent_client"}' \
  "$BASE_URL/admin/keys" | python -c 'import json,sys; print(json.load(sys.stdin))')
```

Keep returned key. Later listing shows metadata only, not raw key.

6. Compile Python protobuf module when needed:

```bash
python -m grpc_tools.protoc -Iproto --python_out=/tmp proto/dailytip.proto
```

7. Request tips:

```bash
PYTHONPATH=/tmp python - <<'PY'
import os
import requests
import dailytip_pb2

base_url = "http://127.0.0.1:3001"
api_key = os.environ["API_KEY"]

req = dailytip_pb2.TipsQuery(count=2, topics="rust, python")
res = requests.post(
    f"{base_url}/tips",
    headers={
        "Authorization": api_key,
        "Content-Type": "application/x-protobuf",
        "Accept": "application/x-protobuf",
    },
    data=req.SerializeToString(),
)
print(res.status_code)
res.raise_for_status()

out = dailytip_pb2.TipsResponse()
out.ParseFromString(res.content)
for tip in out.tips:
    print(tip.id, tip.topic, tip.compressed_content)
PY
```

8. Review card:

```bash
PYTHONPATH=/tmp python - <<'PY'
import os
import requests
import dailytip_pb2

base_url = "http://127.0.0.1:3001"
api_key = os.environ["API_KEY"]
card_id = int(os.environ["CARD_ID"])

req = dailytip_pb2.ReviewPayload(card_id=card_id, grade=4)
res = requests.post(
    f"{base_url}/review",
    headers={
        "Authorization": api_key,
        "Content-Type": "application/x-protobuf",
    },
    data=req.SerializeToString(),
)
print(res.status_code)
res.raise_for_status()
PY
```

9. Casual card flow:

```bash
PYTHONPATH=/tmp python - <<'PY'
import os
import requests
import dailytip_pb2

base_url = "http://127.0.0.1:3001"
api_key = os.environ["API_KEY"]

req = dailytip_pb2.TipsQuery(
    count=1,
    topics="rust",
    topic_class="casual",
    tipcard_type="casual_tip",
)
res = requests.post(
    f"{base_url}/tips",
    headers={
        "Authorization": api_key,
        "Content-Type": "application/x-protobuf",
        "Accept": "application/x-protobuf",
    },
    data=req.SerializeToString(),
)
res.raise_for_status()
out = dailytip_pb2.TipsResponse()
out.ParseFromString(res.content)
card_id = out.tips[0].id

review = dailytip_pb2.ReviewPayload(card_id=card_id, action="acknowledge")
res = requests.post(
    f"{base_url}/review",
    headers={
        "Authorization": api_key,
        "Content-Type": "application/x-protobuf",
    },
    data=review.SerializeToString(),
)
print(res.status_code)
res.raise_for_status()
PY
```

10. Repeatable card flow:

```bash
PYTHONPATH=/tmp python - <<'PY'
import os
import requests
import dailytip_pb2

base_url = "http://127.0.0.1:3001"
api_key = os.environ["API_KEY"]

req = dailytip_pb2.TipsQuery(
    count=1,
    topics="spanish verbs",
    topic_class="re:word",
    tipcard_type="repeatable_tip",
)
res = requests.post(
    f"{base_url}/tips",
    headers={
        "Authorization": api_key,
        "Content-Type": "application/x-protobuf",
        "Accept": "application/x-protobuf",
    },
    data=req.SerializeToString(),
)
res.raise_for_status()
out = dailytip_pb2.TipsResponse()
out.ParseFromString(res.content)
card_id = out.tips[0].id

review = dailytip_pb2.ReviewPayload(card_id=card_id, action="dismiss")
res = requests.post(
    f"{base_url}/review",
    headers={
        "Authorization": api_key,
        "Content-Type": "application/x-protobuf",
    },
    data=review.SerializeToString(),
)
print(res.status_code)
res.raise_for_status()
PY
```

11. List topics through client API:

```bash
PYTHONPATH=/tmp python - <<'PY'
import os
import requests
import dailytip_pb2

res = requests.get(
    "http://127.0.0.1:3001/topics",
    headers={
        "Authorization": os.environ["API_KEY"],
        "Accept": "application/x-protobuf",
    },
)
print(res.status_code)
res.raise_for_status()

out = dailytip_pb2.GetTopicsResponse()
out.ParseFromString(res.content)
print(list(out.topics))
PY
```

## Common Failures

- `401` on `/tips`, `/topics`, `/review`: missing raw `Authorization` key, wrong key, or used `Bearer`.
- `401` on `/admin/*`: no cookie or login failed.
- `400` on `/tips` or `/review`: invalid protobuf body.
- `404` on `/review`: `card_id` has no `review_states` row.
- Tip text says `API KEY MISSING`: `llm_api_key` empty in settings.
- Tip text says `LLM Error: ...`: provider/base URL/model/key problem.

## SRS Grade Meaning

- `0`, `1`, `2`: fail. SM-2 repetitions reset, interval becomes 1 day.
- `3`, `4`, `5`: pass. SM-2 interval advances.
- Current default algorithm for new cards: SM-2.
- FSRS branch exists but placeholder only.

## Queue Action Meaning

- `action="acknowledge"`: card marked acknowledged, no future `/tips`.
- `action="repeat"`: card stay active, next due after growing minute delay.
- `action="memorize"`: card marked memorized, no future `/tips`.
- `action="dismiss"`: card marked dismissed, no future `/tips`.
- Next `/tips` for same casual/repeatable class/topic returns another due card or generate new one.
