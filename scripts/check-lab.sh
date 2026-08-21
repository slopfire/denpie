#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$repo_root"

check_dir=$(mktemp -d /tmp/denpie-lab-check.XXXXXX)
cleanup() {
    case "$check_dir" in
        /tmp/denpie-lab-check.*) rm -rf -- "$check_dir" ;;
        *) printf '%s\n' "refusing to remove unexpected path: $check_dir" >&2 ;;
    esac
}
trap cleanup EXIT INT TERM

lab() {
    DENPIE_SKIP_FRONTEND_BUILD=1 cargo run --quiet --bin denpie-lab -- "$@"
}

assert_contains() {
    file=$1
    expected=$2
    if ! grep -Fq -- "$expected" "$file"; then
        printf '%s\n' "expected '$expected' in $file" >&2
        return 1
    fi
}

printf '%s\n' 'lab-check: offline CLI plans'
lab list >"$check_dir/list.txt"
assert_contains "$check_dir/list.txt" 'images      ready'
assert_contains "$check_dir/list.txt" 'algorithms  planned'

lab run images --dry-run --strategy bing_html --strategy bing_html \
    >"$check_dir/images.txt"
assert_contains "$check_dir/images.txt" '5 cases x 1 strategy = 5 runs'

lab run prompts --dry-run >"$check_dir/prompts.txt"
assert_contains "$check_dir/prompts.txt" '5 cases (0 LLM calls)'

lab run cards --dry-run >"$check_dir/cards.txt"
assert_contains "$check_dir/cards.txt" '7 fixtures (0 gallery artifacts)'

printf '%s\n' 'lab-check: unsafe fixture IDs are rejected'
printf '%s\n' '[{"id":"../escape","topic":"Rust","mode":"one_shot","expected":"reject unsafe id"}]' \
    >"$check_dir/unsafe-prompts.json"
if lab run prompts --dry-run --cases "$check_dir/unsafe-prompts.json" \
    >"$check_dir/unsafe.txt" 2>&1; then
    printf '%s\n' 'unsafe prompt ID unexpectedly passed validation' >&2
    exit 1
fi
assert_contains "$check_dir/unsafe.txt" 'is invalid; ids must match'

printf '%s\n' 'lab-check: scorecard comparison'
printf '%s\n' \
    '[{"case_id":1,"strategy":"bing_html","search_or_download":"search","kind":"prepared","bytes":100,"elapsed_ms":20}]' \
    >"$check_dir/baseline.json"
printf '%s\n' \
    '[{"case_id":1,"strategy":"bing_html","search_or_download":"search","kind":"none","bytes":0,"elapsed_ms":12}]' \
    >"$check_dir/candidate.json"
lab compare "$check_dir/baseline.json" "$check_dir/candidate.json" \
    >"$check_dir/compare.txt"
assert_contains "$check_dir/compare.txt" 'outcome changes: 1'
assert_contains "$check_dir/compare.txt" 'elapsed_ms: 20 -> 12 (-8)'

printf '%s\n' 'lab-check: focused Rust and Yew contracts'
DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --quiet --workspace 'lab::'
DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --quiet -p frontend --features lab-ui \
    checked_in_fixtures_map_to_production_cards
DENPIE_SKIP_FRONTEND_BUILD=1 cargo check --quiet -p frontend --features lab-ui

printf '%s\n' 'lab-check: passed'
