use axum::{
    http::StatusCode,
    routing::{delete, get, post},
    Router,
};
use sqlx::SqlitePool;
use std::{path::PathBuf, sync::Arc};
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::services::ServeDir;
use tower_sessions::SessionManagerLayer;
use webauthn_rs::Webauthn;

use crate::{api, auth, dashboard, services};

pub struct AppState {
    pub db: SqlitePool,
    pub settings_path: PathBuf,
    pub settings: services::settings::SettingsService,
    pub api_keys: services::api_keys::ApiKeyService,
    pub reviews: services::review::ReviewService,
    pub webauthn: Arc<Webauthn>,
}

pub fn build_app<S: tower_sessions::session_store::SessionStore + Clone + Send + Sync + 'static>(
    shared_state: Arc<AppState>,
    session_layer: SessionManagerLayer<S>,
) -> Router {
    let static_dir = std::env::var_os("DENPIE_STATIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("static"));
    let frontend_dist = std::env::var_os("DENPIE_FRONTEND_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("frontend/dist"));

    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(10)
            .finish()
            .unwrap(),
    );

    // Keep the protected routes the same
    let protected_routes = Router::new()
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
        .route("/auth/passkeys", get(auth::list_passkeys))
        .route("/auth/passkeys/:id", delete(auth::delete_passkey))
        .route("/auth/passkeys/register/start", post(auth::register_start))
        .route(
            "/auth/passkeys/register/finish",
            post(auth::register_finish),
        )
        .route_layer(axum::middleware::from_fn(auth::require_session));

    let auth_routes = Router::new()
        .route(
            "/me",
            get(auth::me).patch(auth::update_me).delete(auth::delete_me),
        )
        .route("/login", post(auth::login))
        .route("/passkeys/login/start", post(auth::login_passkey_start))
        .route("/passkeys/login/finish", post(auth::login_passkey_finish))
        .route("/logout", post(auth::logout))
        .route("/setup", post(auth::setup))
        .layer(GovernorLayer {
            config: governor_conf.clone(),
        });

    let frontend_serve = ServeDir::new(frontend_dist.clone()).fallback(
        tower_http::services::ServeFile::new(frontend_dist.join("index.html")),
    );

    Router::new()
        .merge(protected_routes)
        .nest("/auth", auth_routes)
        .nest_service("/static", ServeDir::new(static_dir))
        .route("/api", post(api::unified_api))
        .route("/tips", post(not_found))
        .route("/review", post(not_found))
        .route("/topics", get(not_found))
        .route("/topic-classes", get(not_found))
        .route("/admin", get(not_found))
        .fallback_service(frontend_serve)
        .layer(session_layer)
        .with_state(shared_state)
}

async fn not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}
