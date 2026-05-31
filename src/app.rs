use axum::{
    Router,
    body::Body,
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode},
    http::{HeaderValue, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use sqlx::SqlitePool;
use std::{path::PathBuf, sync::Arc};
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tower_sessions::SessionManagerLayer;
use webauthn_rs::Webauthn;

use crate::{api, auth, dashboard, services};

pub struct AppState {
    pub db: SqlitePool,
    pub image_dir: PathBuf,
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
        .route(
            "/app/topics/regenerate-icon",
            post(dashboard::regenerate_topic_icon),
        )
        .route("/app/tips", post(dashboard::app_tips))
        .route("/app/flow-cards", get(dashboard::flow_cards))
        .route("/app/flow-cards/:id", get(dashboard::flow_card_detail))
        .route("/app/tipcard-images/:id", get(serve_tipcard_image))
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
        .route("/service-worker.js", get(service_worker))
        .route("/api", post(api::unified_api))
        .route("/tips", post(not_found))
        .route("/review", post(not_found))
        .route("/topics", get(not_found))
        .route("/topic-classes", get(not_found))
        .route("/admin", get(not_found))
        .fallback_service(frontend_serve)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(cache_headers))
        .layer(session_layer)
        .with_state(shared_state)
}

async fn serve_tipcard_image(
    State(state): State<Arc<AppState>>,
    session: tower_sessions::Session,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, StatusCode> {
    let user = crate::auth::current_user(&state, &session)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let image = crate::db::repositories::tipcards::find_image(&state.db, &user.id, id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let etag = format!("\"tipcard-image-{}-{}\"", image.id, image.byte_size);
    let etag_header =
        HeaderValue::from_str(&etag).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if etag_matches(headers.get(header::IF_NONE_MATCH), &etag) {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("private, no-cache, max-age=0, must-revalidate"),
                ),
                (header::ETAG, etag_header),
            ],
        )
            .into_response());
    }
    let path = state.image_dir.join(&image.storage_path);
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let content_type = HeaderValue::from_str(&image.mime_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-cache, max-age=0, must-revalidate"),
            ),
            (header::ETAG, etag_header),
        ],
        bytes,
    )
        .into_response())
}

async fn not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

async fn service_worker() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/javascript; charset=utf-8"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache, max-age=0, must-revalidate"),
            ),
        ],
        include_str!("../frontend/service-worker.js"),
    )
        .into_response()
}

async fn cache_headers(req: Request<Body>, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    let mut response = next.run(req).await;

    if !response.status().is_success() {
        return response;
    }

    let headers = response.headers_mut();
    if headers.contains_key(header::CACHE_CONTROL) {
        return response;
    }

    headers.append(header::VARY, HeaderValue::from_static("Accept-Encoding"));

    if path.starts_with("/admin/")
        || path.starts_with("/app/")
        || path.starts_with("/auth/")
        || path.starts_with("/api")
    {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        return response;
    }

    if is_frontend_document(&path) {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache, max-age=0, must-revalidate"),
        );
        return response;
    }

    if is_hashed_frontend_asset(&path) {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    } else if path.starts_with("/static/") {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=2592000, stale-while-revalidate=604800"),
        );
    }

    response
}

fn etag_matches(if_none_match: Option<&HeaderValue>, etag: &str) -> bool {
    if_none_match
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate == "*" || candidate == etag)
        })
        .unwrap_or(false)
}

fn is_frontend_document(path: &str) -> bool {
    if path == "/" || path.ends_with("/index.html") {
        return true;
    }

    path.rsplit('/')
        .next()
        .map(|segment| !segment.contains('.'))
        .unwrap_or(false)
}

fn is_hashed_frontend_asset(path: &str) -> bool {
    path.starts_with("/snippets/")
        || ((path.starts_with("/frontend-") || path.contains("/frontend-"))
            && (path.ends_with(".wasm") || path.ends_with(".js") || path.ends_with(".css")))
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn frontend_document_paths_revalidate() {
        assert!(is_frontend_document("/"));
        assert!(is_frontend_document("/flow"));
        assert!(is_frontend_document("/archive/cards"));
        assert!(is_frontend_document("/index.html"));
        assert!(!is_frontend_document("/frontend-abc123.js"));
    }

    #[test]
    fn only_built_frontend_assets_are_treated_as_immutable() {
        assert!(is_hashed_frontend_asset("/frontend-abc123.js"));
        assert!(is_hashed_frontend_asset("/frontend-abc123_bg.wasm"));
        assert!(is_hashed_frontend_asset(
            "/snippets/frontend-aebe9231459ec4d1/src/passkeys.js"
        ));
        assert!(!is_hashed_frontend_asset("/service-worker.js"));
    }

    #[test]
    fn etag_header_matches_lists() {
        assert!(etag_matches(
            Some(&HeaderValue::from_static(
                "\"other\", \"tipcard-image-1-20\""
            )),
            "\"tipcard-image-1-20\""
        ));
        assert!(etag_matches(
            Some(&HeaderValue::from_static("*")),
            "\"tipcard-image-1-20\""
        ));
        assert!(!etag_matches(
            Some(&HeaderValue::from_static("\"tipcard-image-2-20\"")),
            "\"tipcard-image-1-20\""
        ));
    }
}
