use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::net::SocketAddr;
use std::path::PathBuf;
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
mod dashboard;
mod db;
mod domain;
mod error;
mod llm;
mod services;
mod srs;
#[cfg(test)]
mod tests;

pub use app::{build_app, AppState};
pub use db::migrations::apply_schema_migrations;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Setup Admin Token
    let data_dir = std::env::var_os("DENPIE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&data_dir)
        .await
        .expect("Failed to create data directory");

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

    let session_store = SqliteStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .expect("Failed to migrate session store");

    let api_key_service = services::api_keys::ApiKeyService::new(pool.clone());
    let review_service = services::review::ReviewService::new(pool.clone());
    let shared_state = Arc::new(AppState {
        db: pool,
        settings_path,
        template_dir: std::env::var_os("DENPIE_TEMPLATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("templates")),
        settings: settings_service,
        api_keys: api_key_service,
        reviews: review_service,
    });
    autoupdate::spawn(shared_state.settings_path.clone());
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // Set to true in prod with HTTPS
        .with_expiry(Expiry::OnInactivity(time::Duration::days(1)));

    let app = app::build_app(shared_state, session_layer);

    let addr = std::env::var("DENPIE_BIND_ADDR")
        .ok()
        .map(|value| SocketAddr::from_str(&value).expect("Invalid DENPIE_BIND_ADDR"))
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 3017)));
    println!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
