#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

PYTHONDONTWRITEBYTECODE=1 python3 scripts/check-api-contract.py "$@"
PYTHONDONTWRITEBYTECODE=1 python3 scripts/generate-api-reference.py --check
PYTHONDONTWRITEBYTECODE=1 \
    python3 -m unittest discover -s scripts/tests -p 'test_api_contract.py'
