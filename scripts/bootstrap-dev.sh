#!/usr/bin/env sh
set -eu

missing=0

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'missing: %s\n' "$1" >&2
    missing=1
  fi
}

printf 'Checking Denpie development prerequisites...\n'

need_cmd cargo
need_cmd rustup
need_cmd protoc
need_cmd sqlite3

if ! command -v trunk >/dev/null 2>&1; then
  printf 'missing: trunk (install with: cargo install trunk --locked)\n' >&2
  missing=1
fi

if ! rustup target list --installed | grep -qx 'wasm32-unknown-unknown'; then
  printf 'missing: wasm32-unknown-unknown target (install with: rustup target add wasm32-unknown-unknown)\n' >&2
  missing=1
fi

if ! command -v just >/dev/null 2>&1; then
  printf 'optional: just (install for one-command workflows)\n' >&2
fi

if ! command -v oha >/dev/null 2>&1; then
  printf 'optional: oha (needed for benches/run_bench.sh)\n' >&2
fi

if ! command -v jq >/dev/null 2>&1; then
  printf 'optional: jq (needed for benches/run_bench.sh reports)\n' >&2
fi

if [ "$missing" -ne 0 ]; then
  cat >&2 <<'EOF'

Install the required tools, then rerun this script.
Typical setup:
  rustup target add wasm32-unknown-unknown
  cargo install trunk --locked
  sudo pacman -S protobuf sqlite just
EOF
  exit 1
fi

printf 'All required development prerequisites are available.\n'
