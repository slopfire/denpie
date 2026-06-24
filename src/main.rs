#![allow(clippy::collapsible_if)]

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;
use tracing_subscriber::EnvFilter;

mod api;
mod app;
mod auth;
mod autoupdate;
mod config;
mod context;
mod daily_refresh;
mod dashboard;
mod db;
mod domain;
mod error;
mod http_client;
mod image_compress;
mod image_store;
mod llm;
mod scheduling;
mod services;
#[cfg(test)]
mod tests;
mod types;

pub use app::{AppState, build_app};
pub use db::migrations::apply_schema_migrations;

#[tokio::main]
async fn main() {
    init_tracing();
    build_frontend_for_cargo_run();

    // Setup Admin Token
    let data_dir = std::env::var_os("DENPIE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&data_dir)
        .await
        .expect("Failed to create data directory");
    let image_dir = std::env::var_os("DENPIE_IMAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("tipcard-images"));
    fs::create_dir_all(&image_dir)
        .await
        .expect("Failed to create image directory");

    let settings_path = data_dir.join("settings.yaml");
    let settings_store = config::SettingsStore::new(settings_path.clone());
    let settings_service = services::settings::SettingsService::new(settings_store);

    // Setup DB
    let db_path = data_dir.join("denpie.db");
    let db_options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(db_options)
        .await
        .expect("Failed to create pool");

    // Init schema
    let schema_path = std::env::var_os("DENPIE_SCHEMA_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("schema.sql"));
    tracing::info!(path = %schema_path.display(), "applying schema");
    db::migrations::apply_schema_file(&pool, &schema_path)
        .await
        .expect("Failed to apply schema.sql");
    tracing::info!("applying compatibility migrations");
    db::migrations::apply_schema_migrations(&pool)
        .await
        .expect("Failed to apply schema migrations");
    if db::repositories::users::setup_allowed(&pool)
        .await
        .expect("Failed to check admin setup state")
    {
        let admin_token = settings_service
            .ensure_admin_token()
            .expect("Failed to ensure admin token");
        tracing::warn!(admin_token = %admin_token, "admin setup token generated");
    }
    image_store::migrate_legacy_images(&pool, &image_dir)
        .await
        .expect("Failed to migrate legacy tipcard images");
    tracing::info!(path = %image_dir.display(), "image store ready");

    let session_store = SqliteStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .expect("Failed to migrate session store");

    let api_key_service = services::api_keys::ApiKeyService::new(pool.clone());
    let review_service = services::review::ReviewService::new(pool.clone());

    let webauthn_setup = config::webauthn::setup();

    let frontend_dist = std::env::var_os("DENPIE_FRONTEND_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("frontend/dist"));

    let shared_state = Arc::new(AppState {
        db: pool,
        image_dir,
        settings_path,
        frontend_dist,
        settings: settings_service,
        api_keys: api_key_service,
        reviews: review_service,
        webauthn: webauthn_setup.webauthn,
    });
    autoupdate::spawn(shared_state.settings_path.clone());
    daily_refresh::spawn(shared_state.clone());
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(webauthn_setup.session_secure)
        .with_same_site(tower_sessions::cookie::SameSite::Strict)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(1)));

    let app = app::build_app(shared_state, session_layer);

    let addr = std::env::var("DENPIE_BIND_ADDR")
        .ok()
        .map(|value| SocketAddr::from_str(&value).expect("Invalid DENPIE_BIND_ADDR"))
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 3017)));
    config::webauthn::warn_if_passkeys_misconfigured(&addr, &webauthn_setup.rp_origin);
    tracing::info!(%addr, "listening");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("denpie=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

const DEV_FRONTEND_BUILD_STAMP: &str = "debug-v2";

fn build_frontend_for_cargo_run() {
    if !cfg!(debug_assertions) {
        return;
    }
    if std::env::var_os("DENPIE_SKIP_FRONTEND_BUILD").is_some() {
        tracing::info!("skipping frontend build because DENPIE_SKIP_FRONTEND_BUILD is set");
        return;
    }
    if std::env::var_os("DENPIE_FRONTEND_DIST").is_some() {
        return;
    }
    let frontend_dir = PathBuf::from("frontend");
    if !frontend_dir.join("index.html").exists() {
        return;
    }
    if dev_frontend_build_stamp_matches(&frontend_dir) && frontend_dist_is_fresh(&frontend_dir) {
        tracing::info!("frontend dist is up to date; skipping trunk build");
        return;
    }
    tracing::info!("building frontend with trunk build (debug)");
    let status = Command::new("trunk")
        .arg("build")
        .env_remove("NO_COLOR")
        .current_dir(&frontend_dir)
        .status()
        .unwrap_or_else(|err| {
            panic!("failed to run trunk; install it with `cargo install trunk --locked`: {err}")
        });
    if !status.success() {
        panic!("frontend build failed with status {status}");
    }
    write_dev_frontend_build_stamp(&frontend_dir);
}

fn dev_frontend_build_stamp_path(frontend_dir: &Path) -> PathBuf {
    frontend_dir.join(".dev-build-stamp")
}

fn dev_frontend_build_stamp_matches(frontend_dir: &Path) -> bool {
    std::fs::read_to_string(dev_frontend_build_stamp_path(frontend_dir))
        .map(|value| value.trim() == DEV_FRONTEND_BUILD_STAMP)
        .unwrap_or(false)
}

fn write_dev_frontend_build_stamp(frontend_dir: &Path) {
    let _ = std::fs::write(
        dev_frontend_build_stamp_path(frontend_dir),
        DEV_FRONTEND_BUILD_STAMP,
    );
}

fn frontend_dist_is_fresh(frontend_dir: &Path) -> bool {
    let dist_index = frontend_dir.join("dist/index.html");
    let dist_mtime = match dist_index.metadata().and_then(|meta| meta.modified()) {
        Ok(mtime) => mtime,
        Err(_) => return false,
    };

    for path in [
        frontend_dir.join("Cargo.toml"),
        frontend_dir.join("index.html"),
        frontend_dir.join("Trunk.toml"),
        frontend_dir.join("service-worker.js"),
        frontend_dir.join("src/passkeys.js"),
    ] {
        if path.is_file() && file_is_newer_than(&path, dist_mtime) {
            return false;
        }
    }

    match max_mtime_in_dir(&frontend_dir.join("src")) {
        Some(src_mtime) => src_mtime <= dist_mtime,
        None => false,
    }
}

fn file_is_newer_than(path: &Path, threshold: SystemTime) -> bool {
    path.metadata()
        .ok()
        .and_then(|meta| meta.modified().ok())
        .map(|mtime| mtime > threshold)
        .unwrap_or(true)
}

fn max_mtime_in_dir(dir: &Path) -> Option<SystemTime> {
    if !dir.is_dir() {
        return None;
    }

    let mut stack = vec![dir.to_path_buf()];
    let mut latest = None;

    while let Some(path) = stack.pop() {
        let entries = std::fs::read_dir(&path).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let mtime = entry.metadata().ok()?.modified().ok()?;
            latest = Some(match latest {
                Some(current) if current >= mtime => current,
                _ => mtime,
            });
        }
    }

    latest
}
