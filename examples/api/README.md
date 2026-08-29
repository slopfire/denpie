# API v1 examples

These examples use the canonical [`proto/denpie.proto`](../../proto/denpie.proto)
schema and default to `http://127.0.0.1:3017/api/v1`. Override the endpoint with
`DENPIE_URL` and provide `DENPIE_API_KEY=sk_live_...` for authenticated calls.

Never put a real key in source control or command history. The examples read it
from the environment and do not print it.

## curl + protoc

Discovery needs no key:

```bash
examples/api/curl/call.sh examples/api/requests/get-api-info.textproto
```

An authenticated paginated read:

```bash
DENPIE_API_KEY='sk_live_...' \
  examples/api/curl/call.sh examples/api/requests/list-flow-cards.textproto
```

Suggest a prompt from generated-card history (`topic_id` 0 is the global template):

```bash
DENPIE_API_KEY='sk_live_...' \
  examples/api/curl/call.sh examples/api/requests/enhance-prompt-template.textproto
```

For the mutation fixture, replace its demonstration idempotency key when you
intend a new logical create. Re-running the unchanged fixture safely replays the
original result.

```bash
DENPIE_API_KEY='sk_live_...' \
  examples/api/curl/call.sh examples/api/requests/create-document.textproto
```

## Python

Generate the protobuf module once, then run the standard-library HTTP client:

```bash
mkdir -p target/api-examples/python
protoc --proto_path=proto \
  --python_out=target/api-examples/python proto/denpie.proto
PYTHONPATH=target/api-examples/python \
  python3 examples/api/python/denpie_client.py info

DENPIE_API_KEY='sk_live_...' PYTHONPATH=target/api-examples/python \
  python3 examples/api/python/denpie_client.py cards
```

Creating a document generates one UUID idempotency key and keeps it for the
single process attempt:

```bash
DENPIE_API_KEY='sk_live_...' PYTHONPATH=target/api-examples/python \
  python3 examples/api/python/denpie_client.py create-document
```

Persist that UUID with your job record before adding automatic retries.

## TypeScript

The TypeScript client uses protobuf.js reflection, Node's built-in `fetch`, and
64-bit values converted to strings:

```bash
cd examples/api/typescript
npm ci
npm run info
DENPIE_API_KEY='sk_live_...' npm run cards
DENPIE_API_KEY='sk_live_...' npm run create-document
```

## Rust

The Rust example reuses Denpie's generated Prost module and Reqwest dependency:

```bash
cargo run --example api_v1_client -- info
DENPIE_API_KEY='sk_live_...' cargo run --example api_v1_client -- cards
DENPIE_API_KEY='sk_live_...' \
  DENPIE_IDEMPOTENCY_KEY='replace-with-a-persisted-uuid' \
  cargo run --example api_v1_client -- create-document
```

All four examples have an offline self-test exercised by `just docs-check` and
CI. The self-tests encode and decode real messages without requiring a running
server or credentials.
