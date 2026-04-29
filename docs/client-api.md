# Daily Tip Client API

Base URL for local development:

```text
http://127.0.0.1:3001
```

The client API is protobuf over HTTP. Every route in this file requires a client API key in the raw `Authorization` header.

```http
Authorization: sk_live_your_client_key
Content-Type: application/x-protobuf
Accept: application/x-protobuf
```

The server does not use `Bearer`; send the API key as the full header value.

## Protobuf Schema

Canonical schema: [`proto/dailytip.proto`](../proto/dailytip.proto)

```proto
syntax = "proto3";

package dailytip;

message TipsQuery {
  uint64 count = 1;
  string topics = 2;
  string topic_class = 3;
  string tipcard_type = 4;
}

message TipCardResponse {
  int64 id = 1;
  string topic = 2;
  string full_content = 3;
  string compressed_content = 4;
  string topic_class = 5;
  string tipcard_type = 6;
}

message TipsResponse {
  repeated TipCardResponse tips = 1;
}

message ReviewPayload {
  int64 card_id = 1;
  uint32 grade = 2;
  string action = 3;
}

message GetTopicsResponse {
  repeated string topics = 1;
}

message TopicClass {
  int64 id = 1;
  string name = 2;
  string tipcard_type = 3;
}

message GetTopicClassesResponse {
  repeated TopicClass classes = 1;
}
```

## Authentication

Client keys are created through the admin API or dashboard. The raw key is returned once, then stored only as a SHA-256 hash in SQLite.

Failure responses:

| Case | Status |
|---|---:|
| Missing `Authorization` header | `401 Unauthorized` |
| Invalid API key | `401 Unauthorized` |
| Database error during key lookup | `500 Internal Server Error` |

## `POST /tips`

Returns due cards for requested topics. If a topic has no due card, the server generates a new card with the configured LLM, stores it, creates an initial review state, and returns it.

Request body: `TipsQuery`

| Field | Type | Meaning |
|---|---|---|
| `count` | `uint64` | Maximum number of topic entries to process. |
| `topics` | `string` | Comma-separated topic names, for example `rust, python, go`. |
| `topic_class` | `string` | Optional class name. Defaults to `default`. Classes group topics and define card behavior. |
| `tipcard_type` | `string` | Optional type for a new class: `srs_tip`, `casual_tip`, or `repeatable_tip`. Existing class type wins. |

Behavior:

- Topic names are split on commas and trimmed.
- Empty topic names are skipped.
- Only the first `count` topic entries are processed.
- New topics are inserted automatically.
- New topic classes are inserted automatically. `default` uses `srs_tip`; `casual` defaults to `casual_tip`; `repeatable`, `reword`, and `re:word` default to `repeatable_tip`.
- Existing due cards are selected by earliest `review_states.next_review_at`.
- Generated cards use a topic-specific prompt template when configured in the browser app; otherwise they use the global template. The prompt includes generated titles from existing and dismissed cards for the same topic/type so the model can avoid duplicate ideas.
- `srs_tip` cards use the normal SRS schedule.
- `casual_tip` cards are instant queue cards. Dismiss or acknowledge one, then call `/tips` again to get another card.
- `repeatable_tip` cards are for queue-style practice. If no active due card exists, the server generates a new card immediately. The browser app uses this after repeatable `repeat`, `memorize`, or `dismiss` actions to keep the current slot filled with another card.
- Response order follows the processed topic order.

Response body: `TipsResponse`

| Field | Type | Meaning |
|---|---|---|
| `tips` | repeated `TipCardResponse` | Returned cards. |
| `tips[].id` | `int64` | Card ID. Use this for `/review`. |
| `tips[].topic` | `string` | Topic name used for the card. |
| `tips[].full_content` | `string` | Full LLM-generated tip. |
| `tips[].compressed_content` | `string` | Short compressed version for compact card display. |
| `tips[].topic_class` | `string` | Topic class used for the card. |
| `tips[].tipcard_type` | `string` | Card behavior type: `srs_tip`, `casual_tip`, or `repeatable_tip`. |

Tip text can contain markdown. The protobuf API returns raw strings; clients that display cards should render and sanitize markdown on their side.

Status codes:

| Case | Status |
|---|---:|
| Success | `200 OK` |
| Invalid protobuf decode | `400 Bad Request` |
| Missing/invalid API key | `401 Unauthorized` |
| SQL failure | `500 Internal Server Error` |

## `GET /topics`

Lists all known topic names in ascending order.

Request body: none

Response body: `GetTopicsResponse`

Status codes:

| Case | Status |
|---|---:|
| Success | `200 OK` |
| Missing/invalid API key | `401 Unauthorized` |
| SQL failure | `500 Internal Server Error` |

## `GET /topic-classes`

Lists topic classes and their card behavior type.

Request body: none

Response body: `GetTopicClassesResponse`

| Field | Type | Meaning |
|---|---|---|
| `classes[].id` | `int64` | Topic class ID. |
| `classes[].name` | `string` | Class name, for example `default` or `re:word`. |
| `classes[].tipcard_type` | `string` | `srs_tip`, `casual_tip`, or `repeatable_tip`. |

Status codes:

| Case | Status |
|---|---:|
| Success | `200 OK` |
| Missing/invalid API key | `401 Unauthorized` |
| SQL failure | `500 Internal Server Error` |

## `POST /review`

Records a review grade for a card and advances its SRS state.

Request body: `ReviewPayload`

| Field | Type | Meaning |
|---|---|---|
| `card_id` | `int64` | ID returned by `/tips`. |
| `grade` | `uint32` | Review grade. Current SM-2 logic treats `0`, `1`, and `2` as fail; `3`, `4`, and `5` as pass. |
| `action` | `string` | Optional queue-card action: `acknowledge`, `repeat`, `memorize`, or `dismiss`. Empty action keeps legacy SRS grade behavior. |

Queue-card behavior:

- `action = "acknowledge"` marks the card as acknowledged and removes it from future `/tips` results.
- `action = "repeat"` schedules the card again after a growing delay: 10, 20, 40 minutes, capped at 24 hours.
- `action = "memorize"` marks the card as memorized and removes it from future `/tips` results.
- `action = "dismiss"` marks the card as dismissed and removes it from future `/tips` results.
- After `acknowledge`, `repeat`, `memorize`, or `dismiss`, the next `/tips` request for that class/topic can immediately return another due card or generate a new one. Repeated cards remain active but are not due again until their scheduled delay passes.

Response body: empty

Status codes:

| Case | Status |
|---|---:|
| Review state updated | `200 OK` |
| Invalid protobuf decode | `400 Bad Request` |
| Missing/invalid API key | `401 Unauthorized` |
| No review state exists for `card_id` | `404 Not Found` |
| Invalid stored review state JSON | `500 Internal Server Error` |
| SQL failure | `500 Internal Server Error` |

## Python Example

Compile the protobuf module:

```bash
python -m grpc_tools.protoc -Iproto --python_out=. proto/dailytip.proto
```

Request tips:

```python
import requests
import dailytip_pb2

base_url = "http://127.0.0.1:3001"
api_key = "sk_live_your_client_key"

query = dailytip_pb2.TipsQuery(count=2, topics="rust, python")
res = requests.post(
    f"{base_url}/tips",
    headers={
        "Authorization": api_key,
        "Content-Type": "application/x-protobuf",
        "Accept": "application/x-protobuf",
    },
    data=query.SerializeToString(),
)
res.raise_for_status()

tips = dailytip_pb2.TipsResponse()
tips.ParseFromString(res.content)

for tip in tips.tips:
    print(tip.id, tip.topic, tip.compressed_content)
```

Record a review:

```python
review = dailytip_pb2.ReviewPayload(card_id=tips.tips[0].id, grade=4)
res = requests.post(
    f"{base_url}/review",
    headers={
        "Authorization": api_key,
        "Content-Type": "application/x-protobuf",
    },
    data=review.SerializeToString(),
)
res.raise_for_status()
```

List topics:

```python
res = requests.get(
    f"{base_url}/topics",
    headers={"Authorization": api_key, "Accept": "application/x-protobuf"},
)
res.raise_for_status()

topics = dailytip_pb2.GetTopicsResponse()
topics.ParseFromString(res.content)
print(list(topics.topics))
```

Request repeatable cards:

```python
query = dailytip_pb2.TipsQuery(
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
    data=query.SerializeToString(),
)
res.raise_for_status()

tips = dailytip_pb2.TipsResponse()
tips.ParseFromString(res.content)
card_id = tips.tips[0].id

review = dailytip_pb2.ReviewPayload(card_id=card_id, action="dismiss")
res = requests.post(
    f"{base_url}/review",
    headers={
        "Authorization": api_key,
        "Content-Type": "application/x-protobuf",
    },
    data=review.SerializeToString(),
)
res.raise_for_status()
```

Request casual cards:

```python
query = dailytip_pb2.TipsQuery(
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
    data=query.SerializeToString(),
)
res.raise_for_status()

tips = dailytip_pb2.TipsResponse()
tips.ParseFromString(res.content)
card_id = tips.tips[0].id

review = dailytip_pb2.ReviewPayload(card_id=card_id, action="acknowledge")
res = requests.post(
    f"{base_url}/review",
    headers={
        "Authorization": api_key,
        "Content-Type": "application/x-protobuf",
    },
    data=review.SerializeToString(),
)
res.raise_for_status()
```
