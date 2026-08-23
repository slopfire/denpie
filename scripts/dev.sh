#!/usr/bin/env sh
# Axum backend on :3017 + Astro/Vite on :4321.
# Vite proxies /api, /auth, /app, /admin, and /static to Axum.
# Open http://localhost:4321/ for the live UI.
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if ! command -v bun >/dev/null 2>&1; then
  echo "error: bun is required for just dev" >&2
  exit 1
fi

if [ ! -d frontend-astro/node_modules/astro ]; then
  echo "error: Astro deps missing; run: just frontend-astro-install" >&2
  exit 1
fi

pids=""

cleanup() {
  for pid in $pids; do
    kill "$pid" >/dev/null 2>&1 || true
  done
}

trap cleanup INT TERM EXIT

echo "Astro UI: http://localhost:4321/  (proxies /api /auth /app /admin /static to ${DENPIE_BIND_ADDR:-127.0.0.1:3017})"

DENPIE_SKIP_FRONTEND_BUILD=1 cargo run --bin denpie &
pids="$pids $!"

(cd frontend-astro && bun run dev) &
pids="$pids $!"

wait
