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
need_cmd docker
need_cmd bun

if ! command -v just >/dev/null 2>&1; then
  printf 'optional: just (install for one-command workflows)\n' >&2
fi

if ! command -v oha >/dev/null 2>&1; then
  printf 'optional: oha (needed for benches/run_bench.sh)\n' >&2
fi

if ! command -v jq >/dev/null 2>&1; then
  printf 'optional: jq (needed for benches/run_bench.sh reports)\n' >&2
fi

if ! command -v scrapling >/dev/null 2>&1; then
  printf 'optional: scrapling (main link scraper; pip install "scrapling[fetchers,shell]")\n' >&2
fi

if [ "$missing" -ne 0 ]; then
  cat >&2 <<'EOF'

Install the required tools, then rerun this script.
Typical setup:
  curl -fsSL https://bun.sh/install | bash
  sudo pacman -S protobuf docker docker-compose just
EOF
  exit 1
fi

printf 'All required development prerequisites are available.\n'
