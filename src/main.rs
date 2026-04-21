use axum::{
    routing::{get, post},
    Router,
    middleware::from_fn_with_state,
};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::fs;

mod api;
mod auth;
mod dashboard;
mod llm;
mod srs;

pub struct AppState {
    pub db: SqlitePool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Setup DB
    let db_url = "sqlite://dailytip.db?mode=rwc";
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .expect("Failed to create pool");

    // Init schema
    let schema = fs::read_to_string("schema.sql").await.expect("Failed to read schema.sql");
    for query in schema.split(';') {
        if !query.trim().is_empty() {
            sqlx::query(query).execute(&pool).await.expect("Failed to execute schema");
        }
    }

    let shared_state = Arc::new(AppState { db: pool });

    // Build router
    let api_routes = Router::new()
        .route("/tips", get(api::get_tips))
        .route("/review", post(api::review_card))
        .route_layer(from_fn_with_state(shared_state.clone(), auth::verify_api_key));

    let app = Router::new()
        .merge(api_routes)
        .route("/admin", get(dashboard::index))
        .route("/admin/settings", get(dashboard::get_settings).post(dashboard::update_settings))
        .route("/admin/keys", post(dashboard::create_api_key))
        .with_state(shared_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
