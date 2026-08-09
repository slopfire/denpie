#!/bin/sh
set -eu
umask 077

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
schema="$repo_root/proto/denpie.proto"
endpoint="${DENPIE_URL:-http://127.0.0.1:3017/api/v1}"

if [ "${1:-}" = "--self-test" ]; then
    for request in "$repo_root"/examples/api/requests/*.textproto; do
        protoc --proto_path="$repo_root/proto" \
            --encode=denpie.ApiV1Request "$schema" < "$request" |
            protoc --proto_path="$repo_root/proto" \
                --decode=denpie.ApiV1Request "$schema" >/dev/null
    done
    echo "curl/protoc fixtures encode and decode successfully"
    exit 0
fi

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <request.textproto>" >&2
    echo "       $0 --self-test" >&2
    exit 2
fi

request_file=$1
if [ ! -f "$request_file" ]; then
    echo "request fixture not found: $request_file" >&2
    exit 2
fi

work_dir=$(mktemp -d)
trap 'rm -r "$work_dir"' EXIT HUP INT TERM
request_bin="$work_dir/request.pb"
response_bin="$work_dir/response.pb"

protoc --proto_path="$repo_root/proto" \
    --encode=denpie.ApiV1Request "$schema" < "$request_file" > "$request_bin"

if [ -n "${DENPIE_API_KEY:-}" ]; then
    auth_config="$work_dir/curl-auth.conf"
    printf 'header = "Authorization: Bearer %s"\n' "$DENPIE_API_KEY" > "$auth_config"
    status=$(curl --silent --show-error --output "$response_bin" \
        --write-out '%{http_code}' \
        --config "$auth_config" \
        -H 'Content-Type: application/x-protobuf' \
        --data-binary "@$request_bin" "$endpoint")
else
    status=$(curl --silent --show-error --output "$response_bin" \
        --write-out '%{http_code}' \
        -H 'Content-Type: application/x-protobuf' \
        --data-binary "@$request_bin" "$endpoint")
fi

echo "HTTP $status" >&2
protoc --proto_path="$repo_root/proto" \
    --decode=denpie.ApiV1Response "$schema" < "$response_bin"
