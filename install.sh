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
EOF
}

need_root() {
    if [ "$(id -u)" -ne 0 ]; then
        echo "Run as root, for example: sudo ./install.sh $ACTION" >&2
        exit 1
    fi
}

build_release() {
    if [ "${SKIP_BUILD:-0}" = "1" ]; then
        return
    fi
    cargo build --release
}

ensure_user() {
    if ! getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
        groupadd --system "$SERVICE_GROUP"
    fi
    if ! id "$SERVICE_USER" >/dev/null 2>&1; then
        useradd --system --gid "$SERVICE_GROUP" --home-dir "$DATA_DIR" --create-home --shell /usr/sbin/nologin "$SERVICE_USER"
    fi
}

write_service() {
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
        deploy/dailytipdraft.service > "$SYSTEMD_DIR/$APP_NAME.service"
}

write_autoupdate_defaults() {
    cat > "$DEFAULTS_DIR/$APP_NAME-autoupdate" <<EOF
APP_NAME=$APP_NAME
SERVICE_NAME=$APP_NAME.service
BIN_DIR=$BIN_DIR
SHARE_DIR=$SHARE_DIR
DATA_DIR=$DATA_DIR
SETTINGS_PATH=$DATA_DIR/settings.yaml
STATE_DIR=$DATA_DIR/autoupdate
DEFAULT_REPO=slopfire/dailytipdraft
DEFAULT_BRANCH=main
EOF
}

write_autoupdate_units() {
    install -d -m 0755 "$LIBEXEC_DIR" "$DEFAULTS_DIR"
    install -m 0755 deploy/dailytipdraft-autoupdate.sh "$LIBEXEC_DIR/$APP_NAME-autoupdate"
    sed \
        -e "s|^EnvironmentFile=.*|EnvironmentFile=-$DEFAULTS_DIR/$APP_NAME-autoupdate|" \
        -e "s|^ExecStart=.*|ExecStart=$LIBEXEC_DIR/$APP_NAME-autoupdate|" \
        deploy/dailytipdraft-autoupdate.service > "$SYSTEMD_DIR/$APP_NAME-autoupdate.service"
    install -m 0644 deploy/dailytipdraft-autoupdate.timer "$SYSTEMD_DIR/$APP_NAME-autoupdate.timer"
    write_autoupdate_defaults
}

install_app() {
    need_root
    build_release
    ensure_user

    install -d -m 0755 "$BIN_DIR" "$SHARE_DIR" "$SHARE_DIR/templates"
    install -d -m 0750 -o "$SERVICE_USER" -g "$SERVICE_GROUP" "$DATA_DIR"
    install -m 0755 "target/release/$APP_NAME" "$BIN_DIR/$APP_NAME"
    install -m 0644 schema.sql "$SHARE_DIR/schema.sql"
    install -m 0644 templates/*.html "$SHARE_DIR/templates/"
    write_service
    write_autoupdate_units

    systemctl daemon-reload
    systemctl enable --now "$APP_NAME.service"
    systemctl enable --now "$APP_NAME-autoupdate.timer"

    echo "Installed $APP_NAME"
    echo "URL: http://$BIND_ADDR/"
    echo "Logs: journalctl -u $APP_NAME -f"
    echo "Data: $DATA_DIR"
}

uninstall_app() {
    need_root
    systemctl disable --now "$APP_NAME.service" >/dev/null 2>&1 || true
    systemctl disable --now "$APP_NAME-autoupdate.timer" >/dev/null 2>&1 || true
    rm -f "$SYSTEMD_DIR/$APP_NAME.service"
    rm -f "$SYSTEMD_DIR/$APP_NAME-autoupdate.service" "$SYSTEMD_DIR/$APP_NAME-autoupdate.timer"
    systemctl daemon-reload
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
