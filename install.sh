#!/usr/bin/env sh
set -eu

APP_NAME="denpie"
SERVICE_USER="${SERVICE_USER:-denpie}"
SERVICE_GROUP="${SERVICE_GROUP:-$SERVICE_USER}"
BIND_ADDR="${BIND_ADDR:-127.0.0.1:3017}"
RP_ID="${RP_ID:-denpie.com}"
RP_ORIGIN="${RP_ORIGIN:-https://denpie.com}"
BIN_DIR="${BIN_DIR:-/usr/local/bin}"
SHARE_DIR="${SHARE_DIR:-/usr/local/share/$APP_NAME}"
DATA_DIR="${DATA_DIR:-/var/lib/$APP_NAME}"
SYSTEMD_DIR="${SYSTEMD_DIR:-/etc/systemd/system}"
LIBEXEC_DIR="${LIBEXEC_DIR:-/usr/local/libexec}"
DEFAULTS_DIR="${DEFAULTS_DIR:-/etc/default}"
POLKIT_DIR="${POLKIT_DIR:-/etc/polkit-1/rules.d}"

usage() {
    cat <<EOF
Usage: ./install.sh [install|uninstall|print-service]

Environment overrides:
  BIND_ADDR       listen address for systemd service (default: 127.0.0.1:3017)
  RP_ID           WebAuthn relying party ID for passkeys (default: denpie.com)
  RP_ORIGIN       WebAuthn relying party origin for passkeys (default: https://denpie.com)
  BIN_DIR         binary install directory (default: /usr/local/bin)
  SHARE_DIR       shared asset install directory (default: /usr/local/share/denpie)
  DATA_DIR        runtime data directory (default: /var/lib/denpie)
  LIBEXEC_DIR     helper script directory (default: /usr/local/libexec)
  SERVICE_USER    system user name (default: denpie)
  SKIP_BUILD=1    install existing target/release/denpie and frontend/dist
  RUSTUP_INIT_URL rustup installer URL (default: https://sh.rustup.rs)
EOF
}

command_exists() {
    command -v "$1" >/dev/null 2>&1
}

require_sudo_access() {
    if [ "$(id -u)" -ne 0 ] && ! command_exists sudo; then
        echo "sudo is required for system installation when not running as root" >&2
        exit 1
    fi
}

run_as_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    else
        sudo "$@"
    fi
}

install_rust_toolchain() {
    export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
    export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
    export PATH="$CARGO_HOME/bin:$PATH"

    if command_exists cargo; then
        return
    fi

    if command_exists rustup; then
        rustup toolchain install stable
        rustup default stable
    elif command_exists curl; then
        curl --proto '=https' --tlsv1.2 -sSf "${RUSTUP_INIT_URL:-https://sh.rustup.rs}" | sh -s -- -y --profile minimal
    elif command_exists wget; then
        wget -qO- "${RUSTUP_INIT_URL:-https://sh.rustup.rs}" | sh -s -- -y --profile minimal
    else
        echo "curl or wget is required to install Rust with rustup" >&2
        exit 1
    fi

    export PATH="$CARGO_HOME/bin:$PATH"
    if ! command_exists cargo; then
        echo "Rust installation completed, but cargo is still not available" >&2
        exit 1
    fi
}

build_release() {
    if [ "${SKIP_BUILD:-0}" = "1" ]; then
        return
    fi
    install_rust_toolchain
    if ! command_exists rustup; then
        echo "rustup is required to install the wasm32 frontend target; install rustup or use SKIP_BUILD=1 with prebuilt frontend/dist" >&2
        exit 1
    fi
    rustup target add wasm32-unknown-unknown
    if ! command_exists trunk; then
        cargo install trunk --locked
    fi
    (cd frontend && trunk build --release)
    cargo build --release --package "$APP_NAME"
}

verify_release_artifacts() {
    if [ ! -x "target/release/$APP_NAME" ]; then
        echo "missing target/release/$APP_NAME; run without SKIP_BUILD=1 or build the server first" >&2
        exit 1
    fi
    if [ ! -f frontend/dist/index.html ]; then
        echo "missing frontend/dist/index.html; run without SKIP_BUILD=1 or build the frontend with trunk first" >&2
        exit 1
    fi
}

ensure_user() {
    if ! getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
        run_as_root groupadd --system "$SERVICE_GROUP"
    fi
    if ! id "$SERVICE_USER" >/dev/null 2>&1; then
        run_as_root useradd --system --gid "$SERVICE_GROUP" --home-dir "$DATA_DIR" --create-home --shell /usr/sbin/nologin "$SERVICE_USER"
    fi
}

repair_data_permissions() {
    run_as_root install -d -m 0750 -o "$SERVICE_USER" -g "$SERVICE_GROUP" "$DATA_DIR"
    run_as_root chown -R "$SERVICE_USER:$SERVICE_GROUP" "$DATA_DIR"
    run_as_root find "$DATA_DIR" -type d -exec chmod 0750 {} +
    run_as_root find "$DATA_DIR" -type f -exec chmod u+rw,g-rwx,o-rwx {} +
}

write_service() {
    tmp_file="$(mktemp)"
    sed \
        -e "s|^User=.*|User=$SERVICE_USER|" \
        -e "s|^Group=.*|Group=$SERVICE_GROUP|" \
        -e "s|^WorkingDirectory=.*|WorkingDirectory=$DATA_DIR|" \
        -e "s|^Environment=DENPIE_BIND_ADDR=.*|Environment=DENPIE_BIND_ADDR=$BIND_ADDR|" \
        -e "s|^Environment=DENPIE_RP_ID=.*|Environment=DENPIE_RP_ID=$RP_ID|" \
        -e "s|^Environment=DENPIE_RP_ORIGIN=.*|Environment=DENPIE_RP_ORIGIN=$RP_ORIGIN|" \
        -e "s|^Environment=DENPIE_DATA_DIR=.*|Environment=DENPIE_DATA_DIR=$DATA_DIR|" \
        -e "s|^Environment=DENPIE_SCHEMA_PATH=.*|Environment=DENPIE_SCHEMA_PATH=$SHARE_DIR/schema.sql|" \
        -e "s|^Environment=DENPIE_FRONTEND_DIST=.*|Environment=DENPIE_FRONTEND_DIST=$SHARE_DIR/frontend/dist|" \
        -e "s|^Environment=DENPIE_STATIC_DIR=.*|Environment=DENPIE_STATIC_DIR=$SHARE_DIR/static|" \
        -e "s|^ExecStart=.*|ExecStart=$BIN_DIR/$APP_NAME|" \
        -e "s|^ReadWritePaths=.*|ReadWritePaths=$DATA_DIR|" \
        deploy/denpie.service > "$tmp_file"
    run_as_root install -m 0644 "$tmp_file" "$SYSTEMD_DIR/$APP_NAME.service"
    rm -f "$tmp_file"
}

write_autoupdate_defaults() {
    tmp_file="$(mktemp)"
    rust_path="${CARGO_HOME:-$HOME/.cargo}/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    cat > "$tmp_file" <<EOF
APP_NAME=$APP_NAME
SERVICE_NAME=$APP_NAME.service
SERVICE_USER=$SERVICE_USER
SERVICE_GROUP=$SERVICE_GROUP
BIN_DIR=$BIN_DIR
SHARE_DIR=$SHARE_DIR
DATA_DIR=$DATA_DIR
SETTINGS_PATH=$DATA_DIR/settings.yaml
STATE_DIR=$DATA_DIR/autoupdate
PATH=$rust_path
CARGO_HOME=${CARGO_HOME:-$HOME/.cargo}
RUSTUP_HOME=${RUSTUP_HOME:-$HOME/.rustup}
DEFAULT_REPO=slopfire/denpie
DEFAULT_BRANCH=master
EOF
    run_as_root install -m 0644 "$tmp_file" "$DEFAULTS_DIR/$APP_NAME-autoupdate"
    rm -f "$tmp_file"
}

write_autoupdate_units() {
    run_as_root install -d -m 0755 "$LIBEXEC_DIR" "$DEFAULTS_DIR"
    run_as_root install -m 0755 deploy/denpie-autoupdate.sh "$LIBEXEC_DIR/$APP_NAME-autoupdate"
    tmp_file="$(mktemp)"
    sed \
        -e "s|^EnvironmentFile=.*|EnvironmentFile=-$DEFAULTS_DIR/$APP_NAME-autoupdate|" \
        -e "s|^ExecStart=.*|ExecStart=$LIBEXEC_DIR/$APP_NAME-autoupdate force|" \
        deploy/denpie-autoupdate.service > "$tmp_file"
    run_as_root install -m 0644 "$tmp_file" "$SYSTEMD_DIR/$APP_NAME-autoupdate.service"
    rm -f "$tmp_file"
    run_as_root install -m 0644 deploy/denpie-autoupdate.timer "$SYSTEMD_DIR/$APP_NAME-autoupdate.timer"
    write_autoupdate_defaults
    write_autoupdate_policy
}

write_autoupdate_policy() {
    if [ ! -d "$POLKIT_DIR" ]; then
        echo "Polkit rules directory not found; Check Server Now may require autoupdate_command for manual server updates." >&2
        return
    fi

    tmp_file="$(mktemp)"
    cat > "$tmp_file" <<EOF
polkit.addRule(function(action, subject) {
    if (action.id == "org.freedesktop.systemd1.manage-units" &&
        action.lookup("unit") == "$APP_NAME-autoupdate.service" &&
        action.lookup("verb") == "start" &&
        subject.user == "$SERVICE_USER") {
        return polkit.Result.YES;
    }
});
EOF
    run_as_root install -m 0644 "$tmp_file" "$POLKIT_DIR/49-$APP_NAME-autoupdate.rules"
    rm -f "$tmp_file"
}

install_app() {
    require_sudo_access
    build_release
    verify_release_artifacts
    ensure_user

    run_as_root install -d -m 0755 "$BIN_DIR" "$SHARE_DIR" "$SHARE_DIR/frontend" "$SHARE_DIR/static"
    repair_data_permissions
    run_as_root install -m 0755 "target/release/$APP_NAME" "$BIN_DIR/$APP_NAME"
    run_as_root install -m 0644 schema.sql "$SHARE_DIR/schema.sql"
    run_as_root rm -rf "$SHARE_DIR/frontend/dist"
    run_as_root install -d -m 0755 "$SHARE_DIR/frontend/dist"
    run_as_root cp -R frontend/dist/. "$SHARE_DIR/frontend/dist/"
    run_as_root rm -rf "$SHARE_DIR/static"
    run_as_root install -d -m 0755 "$SHARE_DIR/static"
    run_as_root cp -R static/. "$SHARE_DIR/static/"
    write_service
    write_autoupdate_units

    run_as_root systemctl daemon-reload
    run_as_root systemctl enable --now "$APP_NAME.service"
    run_as_root systemctl enable --now "$APP_NAME-autoupdate.timer"

    echo "Installed $APP_NAME"
    echo "URL: http://$BIND_ADDR/"
    echo "API: http://$BIND_ADDR/api"
    echo "Passkey origin: $RP_ORIGIN"
    echo "Logs: journalctl -u $APP_NAME -f"
    echo "Data: $DATA_DIR"
}

uninstall_app() {
    require_sudo_access
    run_as_root systemctl disable --now "$APP_NAME.service" >/dev/null 2>&1 || true
    run_as_root systemctl disable --now "$APP_NAME-autoupdate.timer" >/dev/null 2>&1 || true
    run_as_root rm -f "$SYSTEMD_DIR/$APP_NAME.service"
    run_as_root rm -f "$SYSTEMD_DIR/$APP_NAME-autoupdate.service" "$SYSTEMD_DIR/$APP_NAME-autoupdate.timer"
    run_as_root rm -f "$POLKIT_DIR/49-$APP_NAME-autoupdate.rules"
    run_as_root systemctl daemon-reload
    echo "Removed systemd services. Data left in $DATA_DIR and binary left in $BIN_DIR."
}

ACTION="${1:-install}"
case "$ACTION" in
    install)
        install_app
        ;;
    uninstall)
        uninstall_app
        ;;
    print-service)
        sed \
            -e "s|127.0.0.1:3017|$BIND_ADDR|g" \
            -e "s|^Environment=DENPIE_RP_ID=.*|Environment=DENPIE_RP_ID=$RP_ID|" \
            -e "s|^Environment=DENPIE_RP_ORIGIN=.*|Environment=DENPIE_RP_ORIGIN=$RP_ORIGIN|" \
            deploy/denpie.service
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        usage
        exit 1
        ;;
esac
