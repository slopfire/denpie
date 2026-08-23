#!/usr/bin/env bash
# Drift check for generated protobuf TypeScript. Regenerates into a temporary
# directory (never mutating the tracked output) and fails on any difference.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
gen_dir="$root/frontend-astro/src/generated"
plugin="$root/frontend-astro/node_modules/.bin/protoc-gen-es"

if [ ! -x "$plugin" ]; then
  echo "error: protoc-gen-es not installed; run 'just frontend-astro-install'" >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

protoc \
  --plugin="protoc-gen-es=$plugin" \
  --es_out="$tmp" \
  --es_opt=target=ts \
  -I"$root/proto" "$root/proto/denpie.proto"

if ! diff -ru "$gen_dir" "$tmp"; then
  echo "error: frontend-astro/src/generated drifted from proto/denpie.proto." >&2
  echo "Run 'just frontend-astro-protogen' and commit the regenerated output." >&2
  exit 1
fi
echo "generated protobuf TypeScript is up to date"
