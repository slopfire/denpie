#!/usr/bin/env sh
# Production Astro build into frontend-astro/dist/.
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
cd "$ROOT/frontend-astro"

if ! command -v bun >/dev/null 2>&1; then
  echo "error: bun is required to build the frontend (https://bun.sh)" >&2
  exit 1
fi

if [ ! -d node_modules/astro ]; then
  bun install --frozen-lockfile
fi

bun run build
