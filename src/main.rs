use axum::{
    middleware::from_fn_with_state,
    routing::{get, post},
    Router,
};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::fs;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;

mod api;
mod auth;
mod dashboard;
mod llm;
mod srs;
#[cfg(test)]
mod tests;

pub struct AppState {
    pub db: SqlitePool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Setup Admin Token
    let settings_str = fs::read_to_string("settings.yaml").await.unwrap_or_default();
    let mut settings: serde_yaml::Value = serde_yaml::from_str(&settings_str).unwrap_or(serde_yaml::Value::Mapping(Default::default()));
    if !settings.is_mapping() {
        settings = serde_yaml::Value::Mapping(Default::default());
    }
    let admin_token = if let Some(token) = settings.get("admin_token").and_then(|v| v.as_str()) {
        token.to_string()
    } else {
        use rand::Rng;
        let token: String = rand::thread_rng().sample_iter(&rand::distributions::Alphanumeric).take(24).map(char::from).collect();
        if let serde_yaml::Value::Mapping(ref mut map) = settings {
            map.insert(serde_yaml::Value::String("admin_token".to_string()), serde_yaml::Value::String(token.clone()));
        }
        let out_str = serde_yaml::to_string(&settings).unwrap();
        fs::write("settings.yaml", out_str).await.unwrap();
        token
    };
    println!(">>> ADMIN SETUP TOKEN: {} <<<", admin_token);

    // Setup DB
    let db_url = "sqlite://dailytip.db?mode=rwc";
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .expect("Failed to create pool");

    // Init schema
    let schema = fs::read_to_string("schema.sql")
        .await
        .expect("Failed to read schema.sql");
    for query in schema.split(';') {
        if !query.trim().is_empty() {
            sqlx::query(query)
                .execute(&pool)
                .await
                .expect("Failed to execute schema");
        }
    }

    let session_store = SqliteStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .expect("Failed to migrate session store");

    let shared_state = Arc::new(AppState {
        db: pool,
    });
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // Set to true in prod with HTTPS
        .with_expiry(Expiry::OnInactivity(time::Duration::days(1)));

    let app = build_app(shared_state, session_layer);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    println!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

pub fn build_app<S: tower_sessions::session_store::SessionStore + Clone + Send + Sync + 'static>(
    shared_state: Arc<AppState>,
    session_layer: SessionManagerLayer<S>,
) -> Router {
    let api_routes = Router::new()
        .route("/tips", post(api::get_tips))
        .route("/review", post(api::review_card))
        .route_layer(from_fn_with_state(
            shared_state.clone(),
            auth::verify_api_key,
        ));

    let admin_routes = Router::new()
        .route(
            "/admin/settings",
            get(dashboard::get_settings).post(dashboard::update_settings),
        )
        .route(
            "/admin/keys",
            get(dashboard::list_api_keys)
                .post(dashboard::create_api_key)
                .delete(dashboard::delete_api_key)
        )
        .route_layer(axum::middleware::from_fn(auth::require_session));

    Router::new()
        .merge(api_routes)
        .merge(admin_routes)
        .route("/admin", get(dashboard::index))
        .route("/auth/login", post(auth::login))
        .layer(session_layer)
        .with_state(shared_state)
}
