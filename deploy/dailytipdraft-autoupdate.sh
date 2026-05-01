#!/usr/bin/env sh
set -eu

APP_NAME="${APP_NAME:-dailytipdraft}"
SERVICE_NAME="${SERVICE_NAME:-dailytipdraft.service}"
BIN_DIR="${BIN_DIR:-/usr/local/bin}"
SHARE_DIR="${SHARE_DIR:-/usr/local/share/$APP_NAME}"
DATA_DIR="${DATA_DIR:-/var/lib/$APP_NAME}"
SETTINGS_PATH="${SETTINGS_PATH:-$DATA_DIR/settings.yaml}"
STATE_DIR="${STATE_DIR:-$DATA_DIR/autoupdate}"
SOURCE_DIR="${SOURCE_DIR:-$STATE_DIR/source}"
DEFAULT_REPO="${DEFAULT_REPO:-slopfire/dailytipdraft}"
DEFAULT_BRANCH="${DEFAULT_BRANCH:-master}"
DEFAULT_INTERVAL_SECS="${DEFAULT_INTERVAL_SECS:-3600}"

log() {
    printf '%s %s\n' "$(date -Is)" "$*"
}

get_yaml_value() {
    key="$1"
    if [ ! -f "$SETTINGS_PATH" ]; then
        return 0
    fi
    sed -n "s/^[[:space:]]*$key:[[:space:]]*//p" "$SETTINGS_PATH" \
        | tail -n 1 \
        | sed 's/[[:space:]]*$//' \
        | sed 's/^"//; s/"$//; s/^'\''//; s/'\''$//'
}

set_yaml_value() {
    key="$1"
    value="$2"
    if [ ! -f "$SETTINGS_PATH" ]; then
        printf '%s: %s\n' "$key" "$value" > "$SETTINGS_PATH"
        return
    fi
    if grep -q "^[[:space:]]*$key:" "$SETTINGS_PATH"; then
        tmp="$SETTINGS_PATH.tmp.$$"
        sed "s|^[[:space:]]*$key:.*|$key: $value|" "$SETTINGS_PATH" > "$tmp"
        cat "$tmp" > "$SETTINGS_PATH"
        rm -f "$tmp"
    else
        printf '\n%s: %s\n' "$key" "$value" >> "$SETTINGS_PATH"
    fi
}

need_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        log "missing required command: $1"
        exit 1
    fi
}

normalize_repo() {
    repo="$1"
    repo="${repo#https://github.com/}"
    repo="${repo#http://github.com/}"
    repo="${repo#git@github.com:}"
    case "$repo" in
        git@*:*) repo="${repo#*:}" ;;
    esac
    repo="${repo%.git}"
    repo="${repo#/}"
    repo="${repo%/}"
    printf '%s' "$repo"
}

now="$(date +%s)"
enabled="$(get_yaml_value autoupdate_enabled || true)"
if [ "$enabled" != "true" ]; then
    log "autoupdate disabled"
    exit 0
fi

repo="$(get_yaml_value autoupdate_repo || true)"
repo="$(normalize_repo "${repo:-$DEFAULT_REPO}")"
branch="$(get_yaml_value autoupdate_branch || true)"
branch="${branch:-$DEFAULT_BRANCH}"
interval="$(get_yaml_value autoupdate_check_interval_secs || true)"
interval="${interval:-$DEFAULT_INTERVAL_SECS}"
case "$interval" in
    ''|*[!0-9]*) interval="$DEFAULT_INTERVAL_SECS" ;;
esac
if [ "$interval" -lt 60 ]; then
    interval=60
fi

mkdir -p "$STATE_DIR"
last_check_file="$STATE_DIR/last_check"
if [ "${1:-}" != "force" ] && [ -f "$last_check_file" ]; then
    last_check="$(cat "$last_check_file" 2>/dev/null || printf '0')"
    case "$last_check" in
        ''|*[!0-9]*) last_check=0 ;;
    esac
    elapsed=$((now - last_check))
    if [ "$elapsed" -lt "$interval" ]; then
        log "autoupdate interval not reached"
        exit 0
    fi
fi
printf '%s\n' "$now" > "$last_check_file"

need_command git
need_command cargo
need_command install
need_command systemctl

remote_url="https://github.com/$repo.git"
latest_sha="$(git ls-remote "$remote_url" "refs/heads/$branch" | awk '{print $1}' | head -n 1)"
if [ -z "$latest_sha" ]; then
    log "no SHA found for $repo $branch"
    exit 1
fi

last_seen="$(get_yaml_value autoupdate_last_seen_sha || true)"
if [ -z "$last_seen" ]; then
    set_yaml_value autoupdate_last_seen_sha "$latest_sha"
    log "recorded autoupdate baseline ${latest_sha}"
    exit 0
fi

if [ "$last_seen" = "$latest_sha" ]; then
    log "already up to date at ${latest_sha}"
    exit 0
fi

log "updating $APP_NAME from ${last_seen} to ${latest_sha}"
rm -rf "$SOURCE_DIR.tmp"
git clone --depth 1 --branch "$branch" "$remote_url" "$SOURCE_DIR.tmp"
(
    cd "$SOURCE_DIR.tmp"
    cargo build --release
)

install -d -m 0755 "$BIN_DIR" "$SHARE_DIR" "$SHARE_DIR/templates"
install -m 0755 "$SOURCE_DIR.tmp/target/release/$APP_NAME" "$BIN_DIR/$APP_NAME"
install -m 0644 "$SOURCE_DIR.tmp/schema.sql" "$SHARE_DIR/schema.sql"
install -m 0644 "$SOURCE_DIR.tmp/templates/"*.html "$SHARE_DIR/templates/"
rm -rf "$SOURCE_DIR"
mv "$SOURCE_DIR.tmp" "$SOURCE_DIR"
set_yaml_value autoupdate_last_seen_sha "$latest_sha"

log "installed update; restarting $SERVICE_NAME"
systemctl restart "$SERVICE_NAME"
