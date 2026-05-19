use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use tokio::fs;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;

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
mod image_store;
mod llm;
mod scheduling;
mod services;
#[cfg(test)]
mod tests;

pub use app::{build_app, AppState};
pub use db::migrations::apply_schema_migrations;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
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
    let admin_token = settings_service
        .ensure_admin_token()
        .expect("Failed to ensure admin token");
    //todo only on startup
    println!(">>> ADMIN SETUP TOKEN: {} <<<", admin_token);

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
    db::migrations::apply_schema_file(&pool, &schema_path)
        .await
        .expect("Failed to apply schema.sql");
    db::migrations::apply_schema_migrations(&pool)
        .await
        .expect("Failed to apply schema migrations");
    image_store::migrate_legacy_images(&pool, &image_dir)
        .await
        .expect("Failed to migrate legacy tipcard images");

    let session_store = SqliteStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .expect("Failed to migrate session store");

    let api_key_service = services::api_keys::ApiKeyService::new(pool.clone());
    let review_service = services::review::ReviewService::new(pool.clone());

    let rp_origin_str =
        std::env::var("DENPIE_RP_ORIGIN").unwrap_or_else(|_| "http://localhost:3017".to_string());
    let rp_origin = url::Url::parse(&rp_origin_str).expect("Invalid DENPIE_RP_ORIGIN");
    let rp_id = std::env::var("DENPIE_RP_ID").unwrap_or_else(|_| {
        rp_origin
            .host_str()
            .expect("DENPIE_RP_ORIGIN must include a host")
            .to_string()
    });
    let webauthn_builder = webauthn_rs::WebauthnBuilder::new(&rp_id, &rp_origin)
        .expect("Invalid webauthn configuration")
        .append_allowed_origin(&url::Url::parse("https://denpie.com").unwrap())
        .append_allowed_origin(&url::Url::parse("https://www.denpie.com").unwrap());

    let webauthn = Arc::new(
        webauthn_builder
            .build()
            .expect("Invalid webauthn configuration"),
    );

    let shared_state = Arc::new(AppState {
        db: pool,
        image_dir,
        settings_path,
        settings: settings_service,
        api_keys: api_key_service,
        reviews: review_service,
        webauthn,
    });
    autoupdate::spawn(shared_state.settings_path.clone());
    daily_refresh::spawn(shared_state.clone());
    let is_prod = std::env::var("DENPIE_PROD").is_ok();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(is_prod)
        .with_same_site(tower_sessions::cookie::SameSite::Strict)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(1)));

    let app = app::build_app(shared_state, session_layer);

    let addr = std::env::var("DENPIE_BIND_ADDR")
        .ok()
        .map(|value| SocketAddr::from_str(&value).expect("Invalid DENPIE_BIND_ADDR"))
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 3017)));
    println!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

fn build_frontend_for_cargo_run() {
    if !cfg!(debug_assertions) {
        return;
    }
    if std::env::var_os("DENPIE_SKIP_FRONTEND_BUILD").is_some() {
        println!("Skipping frontend build because DENPIE_SKIP_FRONTEND_BUILD is set");
        return;
    }
    if std::env::var_os("DENPIE_FRONTEND_DIST").is_some() {
        return;
    }
    let frontend_dir = PathBuf::from("frontend");
    if !frontend_dir.join("index.html").exists() {
        return;
    }
    println!("Building frontend with trunk build --release...");
    let status = Command::new("trunk")
        .args(["build", "--release"])
        .current_dir(&frontend_dir)
        .status()
        .unwrap_or_else(|err| {
            panic!("failed to run trunk; install it with `cargo install trunk --locked`: {err}")
        });
    if !status.success() {
        panic!("frontend build failed with status {status}");
    }
}
