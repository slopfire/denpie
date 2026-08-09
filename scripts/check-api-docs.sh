#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work_dir=$(mktemp -d)
trap 'rm -r "$work_dir"' EXIT HUP INT TERM

cd "$repo_root"
python3 scripts/generate-api-reference.py --check
python3 scripts/check-api-docs.py

mkdir -p "$work_dir/python"
protoc --proto_path=proto --python_out="$work_dir/python" proto/denpie.proto
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$work_dir/python" \
    python3 examples/api/python/denpie_client.py --self-test

sh examples/api/curl/call.sh --self-test
cargo run --quiet --example api_v1_client -- --self-test
if [ ! -x examples/api/typescript/node_modules/.bin/tsc ]; then
    npm --prefix examples/api/typescript ci --ignore-scripts
fi
npm --prefix examples/api/typescript run check

scripts/package-api-schema.sh "$work_dir/schema/v1"
(
    cd "$work_dir/schema/v1"
    sha256sum --check SHA256SUMS
)

echo "API documentation and examples are in sync"
