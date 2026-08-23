#!/usr/bin/env sh
# Isolated Denpie agent runtime on :3027 only.
# Never touches :3017. Creates ephemeral data, boots server, smoke-checks,
# prints test login, and cleans up on exit (or when stop is requested).
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

AGENT_PORT=3027
AGENT_BIND="127.0.0.1:${AGENT_PORT}"
AGENT_ORIGIN="http://localhost:${AGENT_PORT}"
AGENT_DATA="${DENPIE_AGENT_DATA_DIR:-$ROOT/.agent-data}"
PID_FILE="${AGENT_DATA}/server.pid"
LOG_FILE="${AGENT_DATA}/server.log"
KEEP_DATA=0
SMOKE_ONLY=0
STOP_ONLY=0
ONESHOT=0

usage() {
  cat <<'EOF'
Usage: scripts/agent-server.sh [options]

  --keep-data   Leave .agent-data/ after stop (default: remove on clean exit)
  --smoke       Reuse running server if up; only run smoke checks
  --oneshot     Start, smoke, then stop and exit (for just ui-check)
  --stop        Stop a server started by this script (via pid file)
  --frontend-dist DIR
                Serve an alternative built frontend directory (must contain
                index.html; relative paths resolve against the repo root).
                Default: frontend-astro/dist (auto-built if missing). A custom
                dist is never auto-built.
  -h, --help    Show this help

Binds only 127.0.0.1:3027. Never touches :3017.
Test login after bootstrap: username=test password=23452345
EOF
}

FRONTEND_DIST="$ROOT/frontend-astro/dist"

while [ "$#" -gt 0 ]; do
  arg="$1"
  shift
  case "$arg" in
    --keep-data) KEEP_DATA=1 ;;
    --smoke) SMOKE_ONLY=1 ;;
    --oneshot) ONESHOT=1 ;;
    --stop) STOP_ONLY=1 ;;
    --frontend-dist)
      if [ "$#" -eq 0 ]; then
        echo "--frontend-dist requires a value" >&2; usage >&2; exit 2
      fi
      FRONTEND_DIST_ARG="$1"; shift
      if [ -z "$FRONTEND_DIST_ARG" ]; then
        echo "--frontend-dist requires a non-empty value" >&2; usage >&2; exit 2
      fi ;;
    --frontend-dist=*)
      FRONTEND_DIST_ARG="${arg#--frontend-dist=}"
      if [ -z "$FRONTEND_DIST_ARG" ]; then
        echo "--frontend-dist requires a non-empty value" >&2; usage >&2; exit 2
      fi ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $arg" >&2; usage >&2; exit 2 ;;
  esac
done

# Resolve the selected frontend dist against the repo root; never auto-build it.
if [ -n "${FRONTEND_DIST_ARG:-}" ]; then
  case "$FRONTEND_DIST_ARG" in
    /*) FRONTEND_DIST="$FRONTEND_DIST_ARG" ;;
    *) FRONTEND_DIST="$ROOT/$FRONTEND_DIST_ARG" ;;
  esac
fi
CUSTOM_FRONTEND_DIST=0
if [ "$FRONTEND_DIST" != "$ROOT/frontend-astro/dist" ]; then
  CUSTOM_FRONTEND_DIST=1
fi


port_in_use() {
  if command -v ss >/dev/null 2>&1; then
    ss -ltn "sport = :${AGENT_PORT}" 2>/dev/null | grep -q LISTEN
  elif command -v lsof >/dev/null 2>&1; then
    lsof -iTCP:"${AGENT_PORT}" -sTCP:LISTEN >/dev/null 2>&1
  else
    # Fallback: try connecting
    (echo >/dev/tcp/127.0.0.1/"${AGENT_PORT}") >/dev/null 2>&1
  fi
}

is_our_server() {
  [ -f "$PID_FILE" ] || return 1
  pid="$(cat "$PID_FILE" 2>/dev/null || true)"
  [ -n "${pid:-}" ] || return 1
  kill -0 "$pid" 2>/dev/null
}

stop_our_server() {
  if [ -f "$PID_FILE" ]; then
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
      echo "stopping agent server pid=$pid"
      kill "$pid" 2>/dev/null || true
      # Wait briefly for exit
      i=0
      while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 30 ]; do
        sleep 0.1
        i=$((i + 1))
      done
      if kill -0 "$pid" 2>/dev/null; then
        kill -9 "$pid" 2>/dev/null || true
      fi
    fi
    rm -f "$PID_FILE"
  fi
}

cleanup() {
  stop_our_server
  if [ "$KEEP_DATA" -eq 0 ] && [ -d "$AGENT_DATA" ]; then
    # Keep only if user asked; default wipe isolated data.
    rm -rf "$AGENT_DATA"
  fi
}

if [ "$STOP_ONLY" -eq 1 ]; then
  stop_our_server
  echo "agent server stopped (if it was ours)"
  exit 0
fi

mkdir -p "$AGENT_DATA"

if [ "$SMOKE_ONLY" -eq 1 ]; then
  if ! port_in_use; then
    echo "no server listening on :${AGENT_PORT}" >&2
    exit 1
  fi
else
  if port_in_use; then
    if is_our_server; then
      echo "reusing agent server already running on :${AGENT_PORT} (pid $(cat "$PID_FILE"))"
    else
      echo "port :${AGENT_PORT} is already in use — reusing existing listener (will not kill it)"
      # Do not manage lifecycle of a pre-existing process.
      KEEP_DATA=1
      trap - INT TERM EXIT
      # Fall through to smoke + instructions
      SMOKE_ONLY=1
    fi
  fi

  if [ "$SMOKE_ONLY" -eq 0 ]; then
    trap cleanup INT TERM EXIT

    # Fixed admin token so bootstrap is deterministic for agents.
    if [ ! -f "$AGENT_DATA/settings.yaml" ]; then
      printf 'admin_token: agent_admin_token\n' >"$AGENT_DATA/settings.yaml"
    fi

    # Ensure frontend dist exists. The default Astro dist is auto-built when
    # missing; a custom --frontend-dist must already be built and fails clearly.
    if [ ! -f "$FRONTEND_DIST/index.html" ]; then
      if [ "$CUSTOM_FRONTEND_DIST" -eq 1 ]; then
        echo "error: --frontend-dist '$FRONTEND_DIST' has no index.html" >&2
        echo "build the frontend first (e.g. just frontend-astro-build)" >&2
        exit 1
      fi
      echo "building frontend (astro) for agent server..."
      sh "$ROOT/scripts/build-frontend.sh"
    fi

    echo "starting denpie on ${AGENT_BIND} with data dir ${AGENT_DATA}"
    if [ -z "${DATABASE_URL:-}" ]; then
      echo "DATABASE_URL is not set; start local PostgreSQL with: just db-up" >&2
      exit 1
    fi
    DENPIE_BIND_ADDR="$AGENT_BIND" \
      DENPIE_DB_SCHEMA=denpie_agent \
      DENPIE_RP_ORIGIN="$AGENT_ORIGIN" \
      DENPIE_RP_ID=localhost \
      DENPIE_DATA_DIR="$AGENT_DATA" \
      DENPIE_IMAGE_DIR="$AGENT_DATA/tipcard-images" \
      DENPIE_FRONTEND_DIST="$FRONTEND_DIST" \
      DENPIE_SKIP_FRONTEND_BUILD=1 \
      cargo run --bin denpie >"$LOG_FILE" 2>&1 &
    echo $! >"$PID_FILE"

    # Wait for listen
    ready=0
    i=0
    while [ "$i" -lt 120 ]; do
      if port_in_use; then
        # Prefer HTTP 200 from root
        if curl -fsS -o /dev/null "$AGENT_ORIGIN/" 2>/dev/null; then
          ready=1
          break
        fi
      fi
      # Fail fast if process died
      if [ -f "$PID_FILE" ] && ! kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
        echo "server exited early; log:" >&2
        tail -n 80 "$LOG_FILE" >&2 || true
        exit 1
      fi
      sleep 0.25
      i=$((i + 1))
    done
    if [ "$ready" -ne 1 ]; then
      echo "server did not become ready on :${AGENT_PORT}; log:" >&2
      tail -n 80 "$LOG_FILE" >&2 || true
      exit 1
    fi
  fi
fi

# Bootstrap test user if needed (idempotent).
bootstrap_user() {
  # Probe session
  code="$(curl -s -o /dev/null -w '%{http_code}' -c "$AGENT_DATA/cookies.txt" -b "$AGENT_DATA/cookies.txt" "$AGENT_ORIGIN/auth/me" || true)"
  if [ "$code" = "200" ]; then
    return 0
  fi

  # Try login first (existing data)
  login_code="$(curl -s -o /dev/null -w '%{http_code}' \
    -c "$AGENT_DATA/cookies.txt" -b "$AGENT_DATA/cookies.txt" \
    -H 'Content-Type: application/json' \
    -d '{"username":"test","password":"23452345"}' \
    "$AGENT_ORIGIN/auth/login" || true)"
  if [ "$login_code" = "200" ]; then
    return 0
  fi

  # Setup first admin as test user
  setup_code="$(curl -s -o /dev/null -w '%{http_code}' \
    -c "$AGENT_DATA/cookies.txt" -b "$AGENT_DATA/cookies.txt" \
    -H 'Content-Type: application/json' \
    -d '{"username":"test","password":"23452345","admin_token":"agent_admin_token"}' \
    "$AGENT_ORIGIN/auth/setup" || true)"
  if [ "$setup_code" = "200" ] || [ "$setup_code" = "409" ]; then
    # After setup or conflict, login
    curl -s -o /dev/null \
      -c "$AGENT_DATA/cookies.txt" -b "$AGENT_DATA/cookies.txt" \
      -H 'Content-Type: application/json' \
      -d '{"username":"test","password":"23452345"}' \
      "$AGENT_ORIGIN/auth/login" || true
  fi
}

bootstrap_user

echo "=== agent server smoke ==="
fail=0
root_code="$(curl -s -o /dev/null -w '%{http_code}' "$AGENT_ORIGIN/" || true)"
echo "GET /           -> $root_code"
[ "$root_code" = "200" ] || fail=1

me_code="$(curl -s -o /dev/null -w '%{http_code}' -b "$AGENT_DATA/cookies.txt" "$AGENT_ORIGIN/auth/me" || true)"
echo "GET /auth/me    -> $me_code"
[ "$me_code" = "200" ] || fail=1

summary_code="$(curl -s -o /dev/null -w '%{http_code}' -b "$AGENT_DATA/cookies.txt" "$AGENT_ORIGIN/app/summary" || true)"
echo "GET /app/summary -> $summary_code"
[ "$summary_code" = "200" ] || fail=1

if [ "$fail" -ne 0 ]; then
  echo "smoke checks failed" >&2
  if [ -f "$LOG_FILE" ]; then
    echo "--- last log lines ---" >&2
    tail -n 40 "$LOG_FILE" >&2 || true
  fi
  exit 1
fi

cat <<EOF

Agent server ready
  URL:      ${AGENT_ORIGIN}
  Bind:     ${AGENT_BIND}
  Data:     ${AGENT_DATA}
  Frontend: ${FRONTEND_DIST}
  Log:      ${LOG_FILE}
  Login:    username=test  password=23452345

Recipes:
  just agent-server          # start + smoke (blocks until Ctrl-C)
  just agent-server --stop   # stop server we started
  just ui-check              # Astro build + one-shot smoke
EOF

if [ "$ONESHOT" -eq 1 ]; then
  echo "oneshot complete; stopping agent server"
  # cleanup trap will stop server and wipe data unless --keep-data
  exit 0
fi

# Foreground hold: wait on server process until interrupted.
if [ -f "$PID_FILE" ]; then
  pid="$(cat "$PID_FILE")"
  echo "server running (pid $pid); Ctrl-C to stop and clean up"
  wait "$pid" || true
fi
