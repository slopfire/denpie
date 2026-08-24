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
assert_contains "$check_dir/images.txt" '5 cases x 1 strategy x 1 samples = 5 runs'

lab run prompts --dry-run >"$check_dir/prompts.txt"
assert_contains "$check_dir/prompts.txt" '5 cases x 1 samples = 5 runs (0 LLM calls)'

lab run cards --dry-run >"$check_dir/cards.txt"
assert_contains "$check_dir/cards.txt" '15 fixtures (0 gallery artifacts)'
assert_contains "$check_dir/cards.txt" '[stacked] topic: English Grammar status: active pinned: false pending_count: 3 images: 4'
assert_contains "$check_dir/cards.txt" '[two-images] topic: Architectural composition status: active pinned: false pending_count: 0 images: 2'
assert_contains "$check_dir/cards.txt" '[three-images] topic: Visual comparison status: active pinned: false pending_count: 0 images: 3'
assert_contains "$check_dir/cards.txt" '[active] topic: English Grammar status: active pinned: false pending_count: 0 images: 1'
assert_contains "$check_dir/cards.txt" '[manual-tip] topic: Field notes status: active pinned: false pending_count: 0 images: 0'
assert_contains "$check_dir/cards.txt" '[custom-tip] topic: Team conventions status: reviewed pinned: true pending_count: 0 images: 0'

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

printf '%s\n' 'lab-check: focused Rust and Astro contracts'
DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --quiet --workspace 'lab::'

cd "$repo_root/frontend-astro"
bun test src/lib/lab-card-state.test.ts
cd "$repo_root"

sh "$repo_root/scripts/build-frontend.sh" >/dev/null
for fixture_id in active pinned reviewed-hold await-refill daily-complete \
    stacked llm-error long-markdown two-images three-images broken-image \
    api-key-missing manual-tip custom-tip casual-tip; do
    if ! grep -q "lab-fixture-$fixture_id" \
        frontend-astro/dist/lab-cards/index.html; then
        printf '%s\n' "Astro lab page is missing fixture $fixture_id" >&2
        exit 1
    fi
done

printf '%s\n' 'lab-check: offline hydrated DOM interactions'
PLAYWRIGHT_BROWSERS_PATH=0 bunx playwright test \
    --config tests/e2e-astro/lab-cards.config.mjs --grep @smoke

printf '%s\n' 'lab-check: baseline/candidate review workbench'
PLAYWRIGHT_BROWSERS_PATH=0 bunx playwright test \
    --config tests/e2e-astro/lab-review.config.mjs

printf '%s\n' 'lab-check: passed'
