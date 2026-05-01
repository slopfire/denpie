#!/usr/bin/env sh
set -eu

APP_NAME="dailytipdraft"
SERVICE_USER="${SERVICE_USER:-dailytipdraft}"
SERVICE_GROUP="${SERVICE_GROUP:-$SERVICE_USER}"
BIND_ADDR="${BIND_ADDR:-127.0.0.1:3001}"
BIN_DIR="${BIN_DIR:-/usr/local/bin}"
SHARE_DIR="${SHARE_DIR:-/usr/local/share/$APP_NAME}"
DATA_DIR="${DATA_DIR:-/var/lib/$APP_NAME}"
SYSTEMD_DIR="${SYSTEMD_DIR:-/etc/systemd/system}"
LIBEXEC_DIR="${LIBEXEC_DIR:-/usr/local/libexec}"
DEFAULTS_DIR="${DEFAULTS_DIR:-/etc/default}"

usage() {
    cat <<EOF
Usage: ./install.sh [install|uninstall|print-service]

Environment overrides:
  BIND_ADDR       listen address for systemd service (default: 127.0.0.1:3001)
  BIN_DIR         binary install directory (default: /usr/local/bin)
  SHARE_DIR       schema install directory (default: /usr/local/share/dailytipdraft)
  DATA_DIR        runtime data directory (default: /var/lib/dailytipdraft)
  LIBEXEC_DIR     helper script directory (default: /usr/local/libexec)
  SERVICE_USER    system user name (default: dailytipdraft)
  SKIP_BUILD=1    install existing target/release/dailytipdraft
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
    cargo build --release
}

ensure_user() {
    if ! getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
        run_as_root groupadd --system "$SERVICE_GROUP"
    fi
    if ! id "$SERVICE_USER" >/dev/null 2>&1; then
        run_as_root useradd --system --gid "$SERVICE_GROUP" --home-dir "$DATA_DIR" --create-home --shell /usr/sbin/nologin "$SERVICE_USER"
    fi
}

write_service() {
    tmp_file="$(mktemp)"
    sed \
        -e "s|^User=.*|User=$SERVICE_USER|" \
        -e "s|^Group=.*|Group=$SERVICE_GROUP|" \
        -e "s|^WorkingDirectory=.*|WorkingDirectory=$DATA_DIR|" \
        -e "s|^Environment=DAILYTIP_BIND_ADDR=.*|Environment=DAILYTIP_BIND_ADDR=$BIND_ADDR|" \
        -e "s|^Environment=DAILYTIP_DATA_DIR=.*|Environment=DAILYTIP_DATA_DIR=$DATA_DIR|" \
        -e "s|^Environment=DAILYTIP_SCHEMA_PATH=.*|Environment=DAILYTIP_SCHEMA_PATH=$SHARE_DIR/schema.sql|" \
        -e "s|^Environment=DAILYTIP_TEMPLATE_DIR=.*|Environment=DAILYTIP_TEMPLATE_DIR=$SHARE_DIR/templates|" \
        -e "s|^ExecStart=.*|ExecStart=$BIN_DIR/$APP_NAME|" \
        -e "s|^ReadWritePaths=.*|ReadWritePaths=$DATA_DIR|" \
        deploy/dailytipdraft.service > "$tmp_file"
    run_as_root install -m 0644 "$tmp_file" "$SYSTEMD_DIR/$APP_NAME.service"
    rm -f "$tmp_file"
}

write_autoupdate_defaults() {
    tmp_file="$(mktemp)"
    rust_path="${CARGO_HOME:-$HOME/.cargo}/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    cat > "$tmp_file" <<EOF
APP_NAME=$APP_NAME
SERVICE_NAME=$APP_NAME.service
BIN_DIR=$BIN_DIR
SHARE_DIR=$SHARE_DIR
DATA_DIR=$DATA_DIR
SETTINGS_PATH=$DATA_DIR/settings.yaml
STATE_DIR=$DATA_DIR/autoupdate
PATH=$rust_path
CARGO_HOME=${CARGO_HOME:-$HOME/.cargo}
RUSTUP_HOME=${RUSTUP_HOME:-$HOME/.rustup}
DEFAULT_REPO=slopfire/dailytipdraft
DEFAULT_BRANCH=main
EOF
    run_as_root install -m 0644 "$tmp_file" "$DEFAULTS_DIR/$APP_NAME-autoupdate"
    rm -f "$tmp_file"
}

write_autoupdate_units() {
    run_as_root install -d -m 0755 "$LIBEXEC_DIR" "$DEFAULTS_DIR"
    run_as_root install -m 0755 deploy/dailytipdraft-autoupdate.sh "$LIBEXEC_DIR/$APP_NAME-autoupdate"
    tmp_file="$(mktemp)"
    sed \
        -e "s|^EnvironmentFile=.*|EnvironmentFile=-$DEFAULTS_DIR/$APP_NAME-autoupdate|" \
        -e "s|^ExecStart=.*|ExecStart=$LIBEXEC_DIR/$APP_NAME-autoupdate force|" \
        deploy/dailytipdraft-autoupdate.service > "$tmp_file"
    run_as_root install -m 0644 "$tmp_file" "$SYSTEMD_DIR/$APP_NAME-autoupdate.service"
    rm -f "$tmp_file"
    run_as_root install -m 0644 deploy/dailytipdraft-autoupdate.timer "$SYSTEMD_DIR/$APP_NAME-autoupdate.timer"
    write_autoupdate_defaults
}

install_app() {
    require_sudo_access
    build_release
    ensure_user

    run_as_root install -d -m 0755 "$BIN_DIR" "$SHARE_DIR" "$SHARE_DIR/templates"
    run_as_root install -d -m 0750 -o "$SERVICE_USER" -g "$SERVICE_GROUP" "$DATA_DIR"
    run_as_root install -m 0755 "target/release/$APP_NAME" "$BIN_DIR/$APP_NAME"
    run_as_root install -m 0644 schema.sql "$SHARE_DIR/schema.sql"
    run_as_root install -m 0644 templates/*.html "$SHARE_DIR/templates/"
    write_service
    write_autoupdate_units

    run_as_root systemctl daemon-reload
    run_as_root systemctl enable --now "$APP_NAME.service"
    run_as_root systemctl enable --now "$APP_NAME-autoupdate.timer"

    echo "Installed $APP_NAME"
    echo "URL: http://$BIND_ADDR/"
    echo "API: http://$BIND_ADDR/api"
    echo "Logs: journalctl -u $APP_NAME -f"
    echo "Data: $DATA_DIR"
}

uninstall_app() {
    require_sudo_access
    run_as_root systemctl disable --now "$APP_NAME.service" >/dev/null 2>&1 || true
    run_as_root systemctl disable --now "$APP_NAME-autoupdate.timer" >/dev/null 2>&1 || true
    run_as_root rm -f "$SYSTEMD_DIR/$APP_NAME.service"
    run_as_root rm -f "$SYSTEMD_DIR/$APP_NAME-autoupdate.service" "$SYSTEMD_DIR/$APP_NAME-autoupdate.timer"
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
        sed "s|127.0.0.1:3001|$BIND_ADDR|g" deploy/dailytipdraft.service
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        usage
        exit 1
        ;;
esac
