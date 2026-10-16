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
UNIT_FILE="${AGENT_DATA}/server.unit"
UNIT_NAME="${DENPIE_AGENT_UNIT:-denpie-agent-3027}"
LOG_FILE="${AGENT_DATA}/server.log"
KEEP_DATA=0
SMOKE_ONLY=0
STOP_ONLY=0
ONESHOT=0
RESTART=0
DETACH=0
SCHEMA="${DENPIE_AGENT_SCHEMA:-denpie_agent}"

usage() {
  cat <<'EOF'
Usage: scripts/agent-server.sh [options]

  --keep-data   Leave .agent-data/ after stop (default: remove on clean exit)
  --smoke       Reuse running server if up; only run smoke checks
  --oneshot     Start, smoke, then stop and exit (for just ui-check)
  --stop        Stop a server started by this script (via pid file)
  --restart     Stop the server this script started (if any), then start a
                fresh one. A listener we did not start is never killed.
  --detach      Start (or restart), smoke, then return with the server left
                running in the background so the caller can keep working.
                Runs as a transient systemd user unit (denpie-agent-3027) so it
                survives the caller exiting; falls back to setsid if systemd is
                unavailable. Pairs with --restart for a one-command restart.
  --frontend-dist DIR
                Serve an alternative built frontend directory (must contain
                index.html; relative paths resolve against the repo root).
                Default: frontend-astro/dist (auto-built if missing). A custom
                dist is never auto-built.
  --schema NAME PostgreSQL schema to serve (default: denpie_agent). Pass
                `public` to serve your own local data with your own login;
                bootstrap and the session smoke check are skipped then, so the
                first run may report /auth/me -> 401 until you log in.
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
    --restart) RESTART=1 ;;
    --detach) DETACH=1; KEEP_DATA=1 ;;
    --schema)
      if [ "$#" -eq 0 ]; then
        echo "--schema requires a value" >&2; usage >&2; exit 2
      fi
      SCHEMA="$1"; shift
      if [ -z "$SCHEMA" ]; then
        echo "--schema requires a non-empty value" >&2; usage >&2; exit 2
      fi ;;
    --schema=*)
      SCHEMA="${arg#--schema=}"
      if [ -z "$SCHEMA" ]; then
        echo "--schema requires a non-empty value" >&2; usage >&2; exit 2
      fi ;;
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

# The isolated agent schema (default) keeps agent work off real local data:
# bootstrap a known test login and require it to pass the session smoke check.
# Any other schema serves whatever is already there with its own accounts.
if [ "$SCHEMA" = "denpie_agent" ]; then
  ISOLATED=1
else
  ISOLATED=0
fi

# Isolated mode keeps its own data dir and image store. Serving a real schema
# uses the same data dir and tipcard-images directory as `just dev`, so settings
# and card images resolve like the normal local app.
if [ "$ISOLATED" -eq 1 ]; then
  DATA_DIR="$AGENT_DATA"
  IMAGE_DIR="${DENPIE_IMAGE_DIR:-$AGENT_DATA/tipcard-images}"
else
  DATA_DIR="${DENPIE_DATA_DIR:-$ROOT}"
  IMAGE_DIR="${DENPIE_IMAGE_DIR:-$ROOT/tipcard-images}"
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
  # A --detach start runs under a transient systemd user unit.
  if [ -f "$UNIT_FILE" ]; then
    unit="$(cat "$UNIT_FILE" 2>/dev/null || true)"
    if [ -n "$unit" ] && command -v systemctl >/dev/null 2>&1; then
      if [ "$(systemctl --user is-active "$unit" 2>/dev/null || true)" = "active" ]; then
        return 0
      fi
    fi
  fi
  [ -f "$PID_FILE" ] || return 1
  pid="$(cat "$PID_FILE" 2>/dev/null || true)"
  [ -n "${pid:-}" ] || return 1
  kill -0 "$pid" 2>/dev/null
}

stop_our_server() {
  # Detached starts live in a transient unit that outlives this script; a
  # process-group kill would not reach it, so stop the unit explicitly.
  if [ -f "$UNIT_FILE" ]; then
    unit="$(cat "$UNIT_FILE" 2>/dev/null || true)"
    if [ -n "$unit" ] && command -v systemctl >/dev/null 2>&1; then
      if [ "$(systemctl --user is-active "$unit" 2>/dev/null || true)" = "active" ]; then
        echo "stopping agent server unit=$unit"
        systemctl --user stop "$unit" 2>/dev/null || true
      fi
      systemctl --user reset-failed "$unit" 2>/dev/null || true
    fi
    rm -f "$UNIT_FILE"
  fi
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

if [ "$RESTART" -eq 1 ]; then
  if is_our_server; then
    echo "restart: stopping agent server pid=$(cat "$PID_FILE")"
    stop_our_server
  elif port_in_use; then
    echo "restart: :${AGENT_PORT} is held by a listener we did not start — not killing it" >&2
  else
    echo "restart: no agent server was running"
  fi
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
    if [ "$ISOLATED" -eq 1 ] && [ ! -f "$AGENT_DATA/settings.yaml" ]; then
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
    # --detach starts the server outside this script's process group and cgroup
    # so it survives the caller (agent tool call, shell) exiting. A plain
    # `setsid` background job is still inside the caller's cgroup, which the
    # harness reaps at turn end, so prefer a transient systemd user unit.
    DETACH_VIA=""
    if [ "$DETACH" -eq 1 ]; then
      if command -v systemd-run >/dev/null 2>&1 \
        && command -v systemctl >/dev/null 2>&1 \
        && systemctl --user show-environment >/dev/null 2>&1; then
        DETACH_VIA="systemd"
      elif command -v setsid >/dev/null 2>&1; then
        DETACH_VIA="setsid"
      fi
    fi

    if [ "$DETACH_VIA" = "systemd" ]; then
      echo "building denpie binary for the detached unit..."
      DENPIE_SKIP_FRONTEND_BUILD=1 cargo build --bin denpie >&2
      : >"$LOG_FILE"
      systemd-run --user --collect --unit="$UNIT_NAME" \
        --description="Denpie isolated agent server (:${AGENT_PORT})" \
        --working-directory="$ROOT" \
        --property=StandardOutput=append:"$LOG_FILE" \
        --property=StandardError=append:"$LOG_FILE" \
        --setenv=DENPIE_BIND_ADDR="$AGENT_BIND" \
        --setenv=DENPIE_DB_SCHEMA="$SCHEMA" \
        --setenv=DENPIE_RP_ORIGIN="$AGENT_ORIGIN" \
        --setenv=DENPIE_RP_ID=localhost \
        --setenv=DENPIE_DATA_DIR="$DATA_DIR" \
        --setenv=DENPIE_IMAGE_DIR="$IMAGE_DIR" \
        --setenv=DENPIE_FRONTEND_DIST="$FRONTEND_DIST" \
        --setenv=DENPIE_SKIP_FRONTEND_BUILD=1 \
        --setenv=DATABASE_URL="$DATABASE_URL" \
        "$ROOT/target/debug/denpie" >&2
      printf '%s\n' "$UNIT_NAME" >"$UNIT_FILE"
      pid=0
      i=0
      while [ "$i" -lt 20 ]; do
        pid="$(systemctl --user show -p MainPID --value "$UNIT_NAME" 2>/dev/null || true)"
        case "$pid" in ''|*[!0-9]*) pid=0 ;; esac
        [ "$pid" -gt 0 ] && break
        sleep 0.25
        i=$((i + 1))
      done
      if [ "$pid" -gt 0 ]; then
        printf '%s\n' "$pid" >"$PID_FILE"
      else
        echo "detached unit did not report a main pid; unit log:" >&2
        tail -n 40 "$LOG_FILE" >&2 || true
        exit 1
      fi
    else
      START_PREFIX=
      if [ "$DETACH_VIA" = "setsid" ]; then
        START_PREFIX=setsid
      fi

      DENPIE_BIND_ADDR="$AGENT_BIND" \
        DENPIE_DB_SCHEMA="$SCHEMA" \
        DENPIE_RP_ORIGIN="$AGENT_ORIGIN" \
        DENPIE_RP_ID=localhost \
        DENPIE_DATA_DIR="$DATA_DIR" \
        DENPIE_IMAGE_DIR="$IMAGE_DIR" \
        DENPIE_FRONTEND_DIST="$FRONTEND_DIST" \
        DENPIE_SKIP_FRONTEND_BUILD=1 \
        $START_PREFIX cargo run --bin denpie >"$LOG_FILE" 2>&1 </dev/null &
      echo $! >"$PID_FILE"
    fi

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
      if [ -f "$PID_FILE" ]; then
        pid_now="$(cat "$PID_FILE" 2>/dev/null || true)"
        case "$pid_now" in
          ''|*[!0-9]*) ;;
          *) if ! kill -0 "$pid_now" 2>/dev/null; then
               echo "server exited early; log:" >&2
               tail -n 80 "$LOG_FILE" >&2 || true
               exit 1
             fi ;;
        esac
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

if [ "$ISOLATED" -eq 1 ]; then
  bootstrap_user
fi

echo "=== agent server smoke ==="
fail=0
root_code="$(curl -s -o /dev/null -w '%{http_code}' "$AGENT_ORIGIN/" || true)"
echo "GET /           -> $root_code"
[ "$root_code" = "200" ] || fail=1

if [ "$ISOLATED" -eq 1 ]; then
  me_code="$(curl -s -o /dev/null -w '%{http_code}' -b "$AGENT_DATA/cookies.txt" "$AGENT_ORIGIN/auth/me" || true)"
  echo "GET /auth/me    -> $me_code"
  [ "$me_code" = "200" ] || fail=1

  summary_code="$(curl -s -o /dev/null -w '%{http_code}' -b "$AGENT_DATA/cookies.txt" "$AGENT_ORIGIN/app/summary" || true)"
  echo "GET /app/summary -> $summary_code"
  [ "$summary_code" = "200" ] || fail=1
else
  # A non-isolated schema has its own accounts; do not bootstrap or assert a
  # session. 401 here is expected until the human logs in with their account.
  me_code="$(curl -s -o /dev/null -w '%{http_code}' "$AGENT_ORIGIN/auth/me" || true)"
  echo "GET /auth/me    -> $me_code (expected 401: log in with your own account)"
fi

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
  Schema:   ${SCHEMA}
  Data:     ${AGENT_DATA}
  Frontend: ${FRONTEND_DIST}
  Log:      ${LOG_FILE}
EOF

if [ "$ISOLATED" -eq 1 ]; then
  echo "  Login:    username=test  password=23452345 (isolated agent account)"
else
  echo "  Login:    your own ${SCHEMA} account (no test user is created here)"
fi

cat <<EOF

Recipes:
  just agent-server            # start + smoke (blocks until Ctrl-C)
  just restart-agent-server    # stop + start fresh, return with it running
  just stop-agent-server       # stop the server we started
  just serve-local             # same, but serve your real data (public schema)
  just ui-check                # Astro build + one-shot smoke
EOF

if [ "$ONESHOT" -eq 1 ]; then
  echo "oneshot complete; stopping agent server"
  # cleanup trap will stop server and wipe data unless --keep-data
  exit 0
fi

if [ "$DETACH" -eq 1 ] && [ -f "$PID_FILE" ]; then
  # Leave the server running outside this script's cgroup and return.
  trap - INT TERM EXIT
  KEEP_DATA=1
  pid="$(cat "$PID_FILE")"
  cat <<EOF
server left running detached (pid ${pid})
  stop:  just stop-agent-server
  again: just restart-agent-server
EOF
  if [ -f "$UNIT_FILE" ]; then
    echo "  unit:  $(cat "$UNIT_FILE") (systemctl --user status $(cat "$UNIT_FILE"))"
  fi
  exit 0
fi

# Foreground hold: wait on server process until interrupted.
if [ -f "$PID_FILE" ]; then
  pid="$(cat "$PID_FILE")"
  echo "server running (pid $pid); Ctrl-C to stop and clean up"
  wait "$pid" || true
fi
