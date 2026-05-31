#!/usr/bin/env bash
set -euo pipefail
# Denpie API Benchmark Runner — oha v1.14.0 compatible

BENCH_DIR=$(mktemp -d)
BENCH_PORT=13917
BENCH_URL="http://127.0.0.1:$BENCH_PORT"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
RESULTS_DIR="$PROJECT_DIR/benches/results"
ADMIN_TOKEN="bench_admin_token_abc"
PASSWORD="test_password_123"
COOKIE_JAR="$BENCH_DIR/cookies.txt"

mkdir -p "$RESULTS_DIR"

cleanup() {
    echo ""
    echo "=== cleanup ==="
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    rm -rf "$BENCH_DIR"
}
trap cleanup EXIT

echo "=== setting up temp bench env ==="
cp "$PROJECT_DIR/schema.sql" "$BENCH_DIR/"

cat > "$BENCH_DIR/settings.yaml" << EOF
admin_token: "$ADMIN_TOKEN"
autoupdate_enabled: false
EOF

export DENPIE_DATA_DIR="$BENCH_DIR"
export DENPIE_BIND_ADDR="127.0.0.1:$BENCH_PORT"
export DENPIE_SKIP_FRONTEND_BUILD=1
export DENPIE_FRONTEND_DIST="$PROJECT_DIR/frontend/dist"
export DENPIE_SCHEMA_PATH="$BENCH_DIR/schema.sql"
export DENPIE_STATIC_DIR="$PROJECT_DIR/static"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-1.95.0}"

mkdir -p "$PROJECT_DIR/static"

echo "=== starting server ==="
cargo run &
SERVER_PID=$!

echo "Waiting for server startup..."
for i in $(seq 1 180); do
    if curl -s -o /dev/null "$BENCH_URL/" 2>/dev/null; then
        echo "server ready (attempt $i)"
        break
    fi
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        echo "server died during startup"
        exit 1
    fi
    sleep 2
done

echo "=== setting up admin user ==="
SETUP_RESP=$(curl -s -w "\n%{http_code}" -X POST "$BENCH_URL/auth/setup" \
    -H "Content-Type: application/json" \
    -d "{\"admin_token\":\"$ADMIN_TOKEN\",\"username\":\"bench_admin\",\"password\":\"$PASSWORD\"}")
SETUP_CODE=$(echo "$SETUP_RESP" | tail -1)
echo "auth/setup: HTTP $SETUP_CODE"

echo "=== seeding benchmark data ==="
sqlite3 "$BENCH_DIR/denpie.db" << 'SQLSEED'
-- Insert 10 topics for bench_admin
INSERT INTO topics (user_id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, icon_id, color_hue)
SELECT id, 'rust', 'repeatable_tip', 'Teach Rust', 3, 'UTC', '03:00', 'strong', 'code', 210 FROM users WHERE username = 'bench_admin';
INSERT INTO topics (user_id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, icon_id, color_hue)
SELECT id, 'python', 'repeatable_tip', 'Teach Python', 2, 'UTC', '03:00', 'strong', 'code', 45 FROM users WHERE username = 'bench_admin';
INSERT INTO topics (user_id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, icon_id, color_hue)
SELECT id, 'go', 'repeatable_tip', 'Teach Go', 2, 'UTC', '03:00', 'medium', 'code', 120 FROM users WHERE username = 'bench_admin';
INSERT INTO topics (user_id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, icon_id, color_hue)
SELECT id, 'javascript', 'repeatable_tip', 'Teach JS', 2, 'UTC', '03:00', 'strong', 'code', 60 FROM users WHERE username = 'bench_admin';
INSERT INTO topics (user_id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, icon_id, color_hue)
SELECT id, 'sql', 'repeatable_tip', 'Teach SQL', 2, 'UTC', '03:00', 'medium', 'code', 180 FROM users WHERE username = 'bench_admin';
INSERT INTO topics (user_id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, icon_id, color_hue)
SELECT id, 'docker', 'repeatable_tip', 'Teach Docker', 2, 'UTC', '03:00', 'medium', 'code', 240 FROM users WHERE username = 'bench_admin';
INSERT INTO topics (user_id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, icon_id, color_hue)
SELECT id, 'kubernetes', 'repeatable_tip', 'Teach K8s', 2, 'UTC', '03:00', 'medium', 'code', 270 FROM users WHERE username = 'bench_admin';
INSERT INTO topics (user_id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, icon_id, color_hue)
SELECT id, 'aws', 'repeatable_tip', 'Teach AWS', 2, 'UTC', '03:00', 'medium', 'code', 30 FROM users WHERE username = 'bench_admin';
INSERT INTO topics (user_id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, icon_id, color_hue)
SELECT id, 'testing', 'repeatable_tip', 'Teach Testing', 2, 'UTC', '03:00', 'strong', 'code', 300 FROM users WHERE username = 'bench_admin';
INSERT INTO topics (user_id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, icon_id, color_hue)
SELECT id, 'security', 'repeatable_tip', 'Teach Security', 2, 'UTC', '03:00', 'strong', 'code', 330 FROM users WHERE username = 'bench_admin';

-- Insert 50 tipcards (5 per topic) with review states
WITH topic_ids AS (SELECT id FROM topics WHERE user_id = (SELECT id FROM users WHERE username = 'bench_admin'))
INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content, image_data, pinned, created_at)
SELECT 
    (SELECT id FROM users WHERE username = 'bench_admin'),
    t.id,
    'repeatable_tip',
    'Card ' || t.id || '-' || n.num,
    'Full content for card ' || t.id || '-' || n.num || ' with detailed explanation of the concept.',
    'Compressed: ' || t.id || '-' || n.num,
    '[]',
    CASE WHEN n.num = 1 THEN 1 ELSE 0 END,
    datetime('now', '-' || (t.id * 5 + n.num) || ' minutes')
FROM topic_ids t
CROSS JOIN (SELECT 1 AS num UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5) n;

-- Insert review states for all tipcards
INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at)
SELECT 
    t.id,
    'sm2',
    '{"repeats": ' || (t.id % 10) || ', "easiness": 2.5, "interval": ' || (t.id % 30) || '}',
    CASE WHEN t.id % 7 = 0 THEN 'acknowledged' WHEN t.id % 5 = 0 THEN 'dismissed' ELSE 'active' END,
    datetime('now', '+' || (t.id % 30) || ' days')
FROM tipcards t
WHERE t.user_id = (SELECT id FROM users WHERE username = 'bench_admin');

-- Insert 2 tipcard_images per card (100 total)
INSERT INTO tipcard_images (user_id, card_id, position, storage_path, mime_type, byte_size)
SELECT 
    t.user_id,
    t.id,
    1,
    'tips/' || t.id || '_1.png',
    'image/png',
    12345
FROM tipcards t
WHERE t.user_id = (SELECT id FROM users WHERE username = 'bench_admin');

INSERT INTO tipcard_images (user_id, card_id, position, storage_path, mime_type, byte_size)
SELECT 
    t.user_id,
    t.id,
    2,
    'tips/' || t.id || '_2.png',
    'image/png',
    23456
FROM tipcards t
WHERE t.user_id = (SELECT id FROM users WHERE username = 'bench_admin');

-- Ensure user_settings exists
INSERT OR IGNORE INTO user_settings (user_id) SELECT id FROM users WHERE username = 'bench_admin';
SQLSEED

echo "Seed data stats:"
sqlite3 "$BENCH_DIR/denpie.db" "SELECT COUNT(*) as topics FROM topics; SELECT COUNT(*) as tipcards FROM tipcards; SELECT COUNT(*) as review_states FROM review_states; SELECT COUNT(*) as images FROM tipcard_images;"

echo "=== logging in (saving cookies) ==="
LOGIN_RESP=$(curl -s -c "$COOKIE_JAR" -w "\n%{http_code}" \
    -X POST "$BENCH_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"bench_admin\",\"password\":\"$PASSWORD\"}")
LOGIN_CODE=$(echo "$LOGIN_RESP" | tail -1)
echo "auth/login: HTTP $LOGIN_CODE"

SESSION_COOKIE=""
if [ -f "$COOKIE_JAR" ]; then
    SESSION_COOKIE=$(awk '$6=="id" {print $7}' "$COOKIE_JAR" | head -1)
fi

if [ -z "$SESSION_COOKIE" ]; then
    echo "FATAL: cannot extract session cookie from jar"
    cat "$COOKIE_JAR" 2>/dev/null || echo "(empty or missing)"
    exit 1
fi

echo "Session: id=...${SESSION_COOKIE: -20}"

echo "=== verifying endpoints ==="
for path in "/app/summary" "/app/topics" "/admin/settings" "/admin/keys" "/auth/me"; do
    code=$(curl -s -o /dev/null -w "%{http_code}" \
        -H "Cookie: id=$SESSION_COOKIE" "$BENCH_URL$path")
    echo "GET $path → HTTP $code"
done

echo ""
echo "=========================================="
echo "  RUNNING BENCHMARKS"
echo "=========================================="

bench() {
    local name="$1"
    local method="$2"
    local path="$3"
    local n="${4:-500}"
    local c="${5:-10}"
    local result_file="$RESULTS_DIR/${name}.json"
    local url="$BENCH_URL$path"

    echo ""
    echo "--- $name ($method $path) n=$n c=$c ---"

    oha -n "$n" -c "$c" --disable-keepalive \
        --method "$method" \
        --output-format json \
        "$url" > "$result_file" 2>/dev/null

    if [ -s "$result_file" ] && jq -e '.summary' "$result_file" >/dev/null 2>&1; then
        local success_rate
        success_rate=$(jq -r '.summary.successRate // 0' "$result_file")
        if [ "$(echo "$success_rate == 0" | bc -l)" -eq 1 ] 2>/dev/null; then
            echo "  WARNING: all requests failed (successRate=0)"
        fi
        jq -r '
            "  p50: \(.latencyPercentiles.p50 // "N/A")ms",
            "  p90: \(.latencyPercentiles.p90 // "N/A")ms",
            "  p95: \(.latencyPercentiles.p95 // "N/A")ms",
            "  p99: \(.latencyPercentiles.p99 // "N/A")ms",
            "  req/s: \(.summary.requestsPerSec // "N/A")",
            "  total: \(.summary.total // "N/A") sec",
            "  successRate: \(.summary.successRate // "N/A")"
        ' "$result_file"
    else
        echo "  (failed or empty result)"
        cat "$result_file" 2>/dev/null | head -c 200 || true
        echo ""
    fi
}

bench_session() {
    local name="$1"
    local method="$2"
    local path="$3"
    local n="${4:-500}"
    local c="${5:-10}"
    local result_file="$RESULTS_DIR/${name}.json"
    local url="$BENCH_URL$path"

    echo ""
    echo "--- $name ($method $path) n=$n c=$c ---"

    oha -n "$n" -c "$c" --disable-keepalive \
        --method "$method" \
        -H "Cookie: id=$SESSION_COOKIE" \
        --output-format json \
        "$url" > "$result_file" 2>/dev/null

    if [ -s "$result_file" ] && jq -e '.summary' "$result_file" >/dev/null 2>&1; then
        local success_rate
        success_rate=$(jq -r '.summary.successRate // 0' "$result_file")
        if [ "$(echo "$success_rate == 0" | bc -l)" -eq 1 ] 2>/dev/null; then
            echo "  WARNING: all requests failed (successRate=0)"
        fi
        jq -r '
            "  p50: \(.latencyPercentiles.p50 // "N/A")ms",
            "  p90: \(.latencyPercentiles.p90 // "N/A")ms",
            "  p95: \(.latencyPercentiles.p95 // "N/A")ms",
            "  p99: \(.latencyPercentiles.p99 // "N/A")ms",
            "  req/s: \(.summary.requestsPerSec // "N/A")",
            "  total: \(.summary.total // "N/A") sec",
            "  successRate: \(.summary.successRate // "N/A")"
        ' "$result_file"
    else
        echo "  (failed or empty result)"
        cat "$result_file" 2>/dev/null | head -c 200 || true
        echo ""
    fi
}

bench_body() {
    local name="$1"
    local method="$2"
    local path="$3"
    local body="$4"
    local n="${5:-100}"
    local c="${6:-5}"
    local result_file="$RESULTS_DIR/${name}.json"
    local url="$BENCH_URL$path"

    echo ""
    echo "--- $name ($method $path) n=$n c=$c ---"

    printf '%s' "$body" | oha -n "$n" -c "$c" --disable-keepalive \
        --method "$method" \
        -H "Cookie: id=$SESSION_COOKIE" \
        -H "Content-Type: application/json" \
        -d @- \
        --output-format json \
        "$url" > "$result_file" 2>/dev/null

    if [ -s "$result_file" ] && jq -e '.summary' "$result_file" >/dev/null 2>&1; then
        local success_rate
        success_rate=$(jq -r '.summary.successRate // 0' "$result_file")
        if [ "$(echo "$success_rate == 0" | bc -l)" -eq 1 ] 2>/dev/null; then
            echo "  WARNING: all requests failed (successRate=0)"
        fi
        jq -r '
            "  p50: \(.latencyPercentiles.p50 // "N/A")ms",
            "  p90: \(.latencyPercentiles.p90 // "N/A")ms",
            "  p95: \(.latencyPercentiles.p95 // "N/A")ms",
            "  p99: \(.latencyPercentiles.p99 // "N/A")ms",
            "  req/s: \(.summary.requestsPerSec // "N/A")",
            "  total: \(.summary.total // "N/A") sec",
            "  successRate: \(.summary.successRate // "N/A")"
        ' "$result_file"
    else
        echo "  (failed or empty result)"
        cat "$result_file" 2>/dev/null | head -c 200 || true
        echo ""
    fi
}

# 1. Static root (no auth)
bench "01_static_root" GET "/"

# 2. WASM file (static, no auth)
bench "02_static_wasm" GET "/static/frontend-cfdafaa14e5d912a_bg.wasm"

# 3-9. Session endpoints
bench_session "03_app_summary"    GET "/app/summary"
bench_session "04_app_topics"     GET "/app/topics"
bench_session "05_admin_settings" GET "/admin/settings"
bench_session "06_admin_keys"     GET "/admin/keys"
bench_session "07_token_spend"    GET "/admin/token-spend"
bench_session "08_admin_tipcards" GET "/admin/tipcards"
bench_session "09_auth_me"        GET "/auth/me"

# 10. Flow cards (N+1 hotspot - list_images per card)
bench_session "10_flow_cards"     GET "/app/flow-cards"

# 11. Login (rate limited - low concurrency)
bench "11_login" POST "/auth/login" 20 2

# 12-13. PATCH endpoints (lower volume)
bench_body "12_admin_tipcards_pin" PATCH "/admin/tipcards" '{"id":1,"pinned":true}' 100 5
bench_body "13_app_topics_update"  PATCH "/app/topics"      '{"id":1,"name":"advanced rust"}' 100 5

# === REPORT ===
echo ""
echo "=========================================="
echo "  BENCHMARK RESULTS SUMMARY"
echo "=========================================="

{
    echo "# Denpie API Benchmark Results"
    echo "Date: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    echo "Server: $BENCH_URL"
    echo "Tool: oha v$(oha --version 2>/dev/null | awk '{print $2}')"
    echo "Build: debug (unoptimized)"
    echo ""

    echo "## Benchmarked Endpoints"
    echo ""
    echo "| # | Endpoint | Method | Path |"
    echo "|---|----------|--------|------|"
    echo "| 01 | Static root (no auth) | GET | / |"
    echo "| 02 | Static WASM (no auth) | GET | /static/*.wasm |"
    echo "| 03 | App summary (auth) | GET | /app/summary |"
    echo "| 04 | App topics (auth) | GET | /app/topics |"
    echo "| 05 | Admin settings (auth) | GET | /admin/settings |"
    echo "| 06 | Admin API keys (auth) | GET | /admin/keys |"
    echo "| 07 | Token spend (auth) | GET | /admin/token-spend |"
    echo "| 08 | Admin tipcards (auth) | GET | /admin/tipcards |"
    echo "| 09 | Auth me (auth) | GET | /auth/me |"
    echo "| 10 | Flow cards - N+1 hotspot (auth) | GET | /app/flow-cards |"
    echo "| 11 | Login (rate limited) | POST | /auth/login |"
    echo "| 12 | Tipcard pin toggle (auth) | PATCH | /admin/tipcards |"
    echo "| 13 | Topic update (auth) | PATCH | /app/topics |"
    echo ""

    echo "## Results"
    echo ""
    echo "| # | Name | p50 (ms) | p90 (ms) | p95 (ms) | p99 (ms) | req/s | success |"
    echo "|---|------|----------|----------|----------|----------|-------|---------|"

    declare -A BENCH_NAMES
    BENCH_NAMES[01_static_root]="Static root"
    BENCH_NAMES[02_static_wasm]="Static WASM"
    BENCH_NAMES[03_app_summary]="App summary"
    BENCH_NAMES[04_app_topics]="App topics"
    BENCH_NAMES[05_admin_settings]="Admin settings"
    BENCH_NAMES[06_admin_keys]="Admin keys"
    BENCH_NAMES[07_token_spend]="Token spend"
    BENCH_NAMES[08_admin_tipcards]="Admin tipcards"
    BENCH_NAMES[09_auth_me]="Auth me"
    BENCH_NAMES[10_flow_cards]="Flow cards"
    BENCH_NAMES[11_login]="Login"
    BENCH_NAMES[12_admin_tipcards_pin]="Tipcard pin"
    BENCH_NAMES[13_app_topics_update]="Topic update"

    for result in "$RESULTS_DIR"/[0-9]*.json; do
        [ -f "$result" ] || continue
        name=$(basename "$result" .json)
        num=${name%%_*}
        label="${BENCH_NAMES[$name]:-$name}"
        if jq -e '.summary' "$result" >/dev/null 2>&1; then
            p50=$(jq -r '(.latencyPercentiles.p50 // 0) * 1000' "$result" | awk '{printf "%.2f", $1}')
            p90=$(jq -r '(.latencyPercentiles.p90 // 0) * 1000' "$result" | awk '{printf "%.2f", $1}')
            p95=$(jq -r '(.latencyPercentiles.p95 // 0) * 1000' "$result" | awk '{printf "%.2f", $1}')
            p99=$(jq -r '(.latencyPercentiles.p99 // 0) * 1000' "$result" | awk '{printf "%.2f", $1}')
            rps=$(jq -r '.summary.requestsPerSec // "N/A"' "$result" | awk '{printf "%.1f", $1}')
            sr=$(jq -r '.summary.successRate // "N/A"' "$result")
            echo "| $num | $label | $p50 | $p90 | $p95 | $p99 | $rps | $sr |"
        else
            echo "| $num | $label | ERR | ERR | ERR | ERR | ERR | ERR |"
        fi
    done

    echo ""
    echo "## Metrics Explained"
    echo ""
    echo "**Latency Percentiles (ms)** — Time from request sent to full response received."
    echo ""
    echo "- **p50 (median)** — Half of all requests were faster than this. Good general indicator of typical performance."
    echo "- **p90** — 90% of requests were faster than this. Catches outliers starting to appear."
    echo "- **p95** — 95% of requests were faster than this. Shows tail latency; good threshold for 'worst acceptable' under load."
    echo "- **p99** — 99% of requests were faster than this. Extreme tail latency; reveals starvation, lock contention, or GC pauses."
    echo ""
    echo "**Throughput & Reliability**"
    echo ""
    echo "- **req/s** — Requests per second. Higher is better. This is total throughput across all concurrent connections used in the test."
    echo "- **success** — Success rate (0.0 to 1.0). 1.0 = every request got HTTP 2xx/3xx. Lower values mean timeouts, errors, or rate-limiting (see benchmark #11)."
    echo ""
    echo "### How to read this report"
    echo ""
    echo "1. Start with **p50** to see typical user-facing latency."
    echo "2. Compare **p50** vs **p99** gap. A large gap (>10x) means inconsistent performance under load — investigate tail latency causes."
    echo "3. Check **success** rate. Anything below 1.0 on non-rate-limited endpoints is a red flag."
    echo "4. **req/s** shows capacity. Compare against your expected traffic. Remember: these are unoptimized debug builds."
    echo "5. **#10 (Flow cards)** is the N+1 hotspot — it fetches images per card. If p99 is high here, that's expected and the target for optimization."
    echo "6. **#11 (Login)** runs at low concurrency (2) because it is rate-limited. Low req/s here is by design."

    echo ""
    echo "## System"
    echo "- CPU: $(lscpu | grep 'Model name' | head -1 | sed 's/Model name:\s*//')"
    echo "- Cores: $(nproc)"
    echo "- RAM: $(free -h | grep Mem | awk '{print $2}')"
    echo "- Rust: $(rustc --version 2>/dev/null || echo 'unknown')"
} > "$RESULTS_DIR/report.md"

echo ""
echo "=== DONE ==="
echo "Results: $RESULTS_DIR/"
echo "Report: $RESULTS_DIR/report.md"

echo ""
if command -v glow >/dev/null 2>&1; then
    glow "$RESULTS_DIR/report.md"
else
    cat "$RESULTS_DIR/report.md"
fi
