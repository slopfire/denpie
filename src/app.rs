use axum::{
    routing::{get, post},
    Router,
};
use sqlx::SqlitePool;
use std::{path::PathBuf, sync::Arc};
use tower_http::services::ServeDir;
use tower_sessions::SessionManagerLayer;

use crate::{api, auth, dashboard, services};

pub struct AppState {
    pub db: SqlitePool,
    pub settings_path: PathBuf,
    pub template_dir: PathBuf,
    pub settings: services::settings::SettingsService,
    pub api_keys: services::api_keys::ApiKeyService,
    pub reviews: services::review::ReviewService,
}

pub fn build_app<S: tower_sessions::session_store::SessionStore + Clone + Send + Sync + 'static>(
    shared_state: Arc<AppState>,
    session_layer: SessionManagerLayer<S>,
) -> Router {
    let static_dir = std::env::var_os("DENPIE_STATIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("static"));

    Router::new()
        .route(
            "/admin/settings",
            get(dashboard::get_settings).post(dashboard::update_settings),
        )
        .route("/admin/autoupdate", post(dashboard::trigger_autoupdate))
        .route(
            "/admin/autoupdate/status",
            get(dashboard::autoupdate_status),
        )
        .route(
            "/admin/keys",
            get(dashboard::list_api_keys)
                .post(dashboard::create_api_key)
                .delete(dashboard::delete_api_key),
        )
        .route("/admin/topics", get(dashboard::list_topics))
        .route("/admin/token-spend", get(dashboard::token_spend))
        .route(
            "/admin/tipcards",
            get(dashboard::list_tipcards)
                .patch(dashboard::pin_tipcard)
                .delete(dashboard::delete_tipcard),
        )
        .route("/app/summary", get(dashboard::app_summary))
        .route(
            "/app/topics",
            get(dashboard::app_topics)
                .patch(dashboard::update_topic)
                .delete(dashboard::delete_topic),
        )
        .route("/app/tips", post(dashboard::app_tips))
        .route("/app/daily-refresh", post(dashboard::force_daily_refresh))
        .route("/app/review", post(dashboard::app_review))
        .route_layer(axum::middleware::from_fn(auth::require_session))
        .nest_service("/static", ServeDir::new(static_dir))
        .route("/", get(dashboard::app_index))
        .route("/auth/me", get(auth::me))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/setup", post(auth::setup))
        .route("/api", post(api::unified_api))
        .layer(session_layer)
        .with_state(shared_state)
}
