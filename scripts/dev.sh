#!/usr/bin/env sh
set -eu

pids=""

cleanup() {
  for pid in $pids; do
    kill "$pid" >/dev/null 2>&1 || true
  done
}

trap cleanup INT TERM EXIT

DENPIE_SKIP_FRONTEND_BUILD=1 cargo run &
pids="$pids $!"

(cd frontend && env -u NO_COLOR trunk watch) &
pids="$pids $!"

wait
