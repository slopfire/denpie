#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output_dir=${1:-"$repo_root/target/api-schema/v1"}

case "$output_dir" in
    ""|/|"$repo_root")
        echo "refusing unsafe schema output directory: $output_dir" >&2
        exit 2
        ;;
esac

mkdir -p "$output_dir"
cp "$repo_root/proto/denpie.proto" "$output_dir/denpie.proto"
protoc --proto_path="$repo_root/proto" \
    --include_imports \
    --include_source_info \
    --descriptor_set_out="$output_dir/denpie.pb" \
    "$repo_root/proto/denpie.proto"

cat > "$output_dir/manifest.json" <<'EOF'
{
  "api_version": "v1",
  "package": "denpie",
  "canonical_schema": "proto/denpie.proto",
  "source": "denpie.proto",
  "descriptor_set": "denpie.pb"
}
EOF

(
    cd "$output_dir"
    sha256sum denpie.proto denpie.pb manifest.json > SHA256SUMS
)

echo "wrote API v1 schema bundle to $output_dir"
