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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use prost::Message;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let schema = tokio::fs::read_to_string("schema.sql")
            .await
            .unwrap_or_default();
        for query in schema.split(';') {
            if !query.trim().is_empty() {
                sqlx::query(query).execute(&pool).await.unwrap();
            }
        }
        pool
    }

    /// Write a test settings.yaml and spin up a real server on an ephemeral port.
    /// Returns (base_url, reqwest::Client with cookie jar).
    async fn spawn_test_server() -> (String, reqwest::Client) {
        let test_token = "test_admin_token_xyz";
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("admin_token".into()),
            serde_yaml::Value::String(test_token.into()),
        );
        let settings_val = serde_yaml::Value::Mapping(map);
        fs::write("settings.yaml", serde_yaml::to_string(&settings_val).unwrap())
            .await
            .unwrap();

        let db = setup_db().await;
        let state = Arc::new(AppState { db });
        let session_store = tower_sessions::MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store).with_secure(false);
        let app = build_app(state, session_layer);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{}", addr);

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        (base_url, client)
    }

    // ──────────────────────────────────────────────
    //  Auth Tests
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_login_success() {
        let (url, client) = spawn_test_server().await;
        let res = client
            .post(format!("{url}/auth/login"))
            .json(&serde_json::json!({ "admin_token": "test_admin_token_xyz" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_login_wrong_token() {
        let (url, client) = spawn_test_server().await;
        let res = client
            .post(format!("{url}/auth/login"))
            .json(&serde_json::json!({ "admin_token": "wrong_token_lol" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_admin_routes_require_session() {
        let (url, client) = spawn_test_server().await;
        // No login performed — all admin routes should 401
        let settings = client.get(format!("{url}/admin/settings")).send().await.unwrap();
        assert_eq!(settings.status(), reqwest::StatusCode::UNAUTHORIZED);

        let keys = client.get(format!("{url}/admin/keys")).send().await.unwrap();
        assert_eq!(keys.status(), reqwest::StatusCode::UNAUTHORIZED);

        let create = client
            .post(format!("{url}/admin/keys"))
            .json(&serde_json::json!({ "client_name": "nope" }))
            .send()
            .await
            .unwrap();
        assert_eq!(create.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    // ──────────────────────────────────────────────
    //  Admin Dashboard HTML
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_admin_page_serves_html() {
        let (url, client) = spawn_test_server().await;
        let res = client.get(format!("{url}/admin")).send().await.unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let body = res.text().await.unwrap();
        assert!(body.contains("DAILY"), "Admin page should contain the app title");
        assert!(body.contains("TIP"), "Admin page should contain the app title part 2");
        assert!(body.contains("adminTokenInput"), "Admin page should contain the login input");
    }

    // ──────────────────────────────────────────────
    //  Settings CRUD
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_and_update_settings() {
        let (url, client) = spawn_test_server().await;

        // Login first
        client
            .post(format!("{url}/auth/login"))
            .json(&serde_json::json!({ "admin_token": "test_admin_token_xyz" }))
            .send()
            .await
            .unwrap();

        // GET defaults
        let res = client.get(format!("{url}/admin/settings")).send().await.unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let data: serde_json::Value = res.json().await.unwrap();
        assert!(data["model"].as_str().is_some());
        assert!(data["template"].as_str().is_some());

        // POST update
        let update_res = client
            .post(format!("{url}/admin/settings"))
            .json(&serde_json::json!({
                "model": "google/gemini-2.5-pro",
                "template": "Tell me a fun fact about {topic}.",
                "api_key": "test-api-key-123",
                "base_url": "https://openrouter.ai/api/v1"
            }))
            .send()
            .await
            .unwrap();
        let update_status = update_res.status();
        let update_body = update_res.text().await.unwrap_or_default();
        assert_eq!(update_status, reqwest::StatusCode::OK, "Settings update failed: body={}", update_body);

        // GET again to verify persistence
        let res = client.get(format!("{url}/admin/settings")).send().await.unwrap();
        let data: serde_json::Value = res.json().await.unwrap();
        assert_eq!(data["model"], "google/gemini-2.5-pro");
        assert_eq!(data["template"], "Tell me a fun fact about {topic}.");
    }

    // ──────────────────────────────────────────────
    //  API Key Management
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_key_create_list_delete() {
        let (url, client) = spawn_test_server().await;
        client
            .post(format!("{url}/auth/login"))
            .json(&serde_json::json!({ "admin_token": "test_admin_token_xyz" }))
            .send()
            .await
            .unwrap();

        // Create two keys
        let key1: String = client
            .post(format!("{url}/admin/keys"))
            .json(&serde_json::json!({ "client_name": "widget" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(key1.starts_with("sk_live_"), "Key should have sk_live_ prefix");

        let _key2: String = client
            .post(format!("{url}/admin/keys"))
            .json(&serde_json::json!({ "client_name": "telegram_bot" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        // List — should be 2
        let keys: Vec<crate::dashboard::ApiKeyInfo> = client
            .get(format!("{url}/admin/keys"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(keys.len(), 2);

        // Delete one
        let id_to_delete = keys.iter().find(|k| k.client_name == "widget").unwrap().id;
        let del = client
            .delete(format!("{url}/admin/keys"))
            .json(&serde_json::json!({ "id": id_to_delete }))
            .send()
            .await
            .unwrap();
        assert_eq!(del.status(), reqwest::StatusCode::OK);

        // List — should be 1
        let keys: Vec<crate::dashboard::ApiKeyInfo> = client
            .get(format!("{url}/admin/keys"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].client_name, "telegram_bot");
    }

    // ──────────────────────────────────────────────
    //  API Key Auth (tips/review endpoints)
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_api_with_invalid_key() {
        let (url, client) = spawn_test_server().await;

        let tips_query = crate::api::pb::TipsQuery {
            count: 1,
            topics: "rust".into(),
        };
        let res = client
            .post(format!("{url}/tips"))
            .header("Authorization", "sk_live_totallyFakeKeyBruh")
            .body(tips_query.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_api_missing_auth_header() {
        let (url, client) = spawn_test_server().await;

        let tips_query = crate::api::pb::TipsQuery {
            count: 1,
            topics: "rust".into(),
        };
        let res = client
            .post(format!("{url}/tips"))
            .body(tips_query.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    // ──────────────────────────────────────────────
    //  Full API Flow: tips → review → review-not-found
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_full_api_flow() {
        let (url, client) = spawn_test_server().await;

        // Login and create a key
        client
            .post(format!("{url}/auth/login"))
            .json(&serde_json::json!({ "admin_token": "test_admin_token_xyz" }))
            .send()
            .await
            .unwrap();

        let api_key: String = client
            .post(format!("{url}/admin/keys"))
            .json(&serde_json::json!({ "client_name": "flow_test" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        // Fetch tips
        let tips_query = crate::api::pb::TipsQuery {
            count: 1,
            topics: "rust".into(),
        };
        let res = client
            .post(format!("{url}/tips"))
            .header("Authorization", &api_key)
            .body(tips_query.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);

        let tips_resp =
            crate::api::pb::TipsResponse::decode(res.bytes().await.unwrap()).unwrap();
        assert_eq!(tips_resp.tips.len(), 1);
        let card_id = tips_resp.tips[0].id;
        assert!(!tips_resp.tips[0].full_content.is_empty());
        assert!(!tips_resp.tips[0].compressed_content.is_empty());
        assert_eq!(tips_resp.tips[0].topic, "rust");

        // Review that card — should succeed
        let review = crate::api::pb::ReviewPayload {
            card_id,
            grade: 4,
        };
        let res = client
            .post(format!("{url}/review"))
            .header("Authorization", &api_key)
            .body(review.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);

        // Review a card that doesn't exist — should 404
        let ghost_review = crate::api::pb::ReviewPayload {
            card_id: 99999,
            grade: 3,
        };
        let res = client
            .post(format!("{url}/review"))
            .header("Authorization", &api_key)
            .body(ghost_review.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
    }

    // ──────────────────────────────────────────────
    //  Protobuf error handling
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_tips_bad_protobuf_body() {
        let (url, client) = spawn_test_server().await;

        // Login and create key
        client
            .post(format!("{url}/auth/login"))
            .json(&serde_json::json!({ "admin_token": "test_admin_token_xyz" }))
            .send()
            .await
            .unwrap();
        let api_key: String = client
            .post(format!("{url}/admin/keys"))
            .json(&serde_json::json!({ "client_name": "garbage_test" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        // Send garbage bytes
        let res = client
            .post(format!("{url}/tips"))
            .header("Authorization", &api_key)
            .body(vec![0xDE, 0xAD, 0xBE, 0xEF])
            .send()
            .await
            .unwrap();
        // Protobuf is lenient, but let's verify it at least doesn't 500
        assert!(
            res.status() == reqwest::StatusCode::OK
                || res.status() == reqwest::StatusCode::BAD_REQUEST,
            "Should handle garbage protobuf gracefully, got {}",
            res.status()
        );
    }

    // ──────────────────────────────────────────────
    //  Multiple topics in one request
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_tips_multiple_topics() {
        let (url, client) = spawn_test_server().await;

        client
            .post(format!("{url}/auth/login"))
            .json(&serde_json::json!({ "admin_token": "test_admin_token_xyz" }))
            .send()
            .await
            .unwrap();
        let api_key: String = client
            .post(format!("{url}/admin/keys"))
            .json(&serde_json::json!({ "client_name": "multi_topic" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let tips_query = crate::api::pb::TipsQuery {
            count: 3,
            topics: "rust, python, go".into(),
        };
        let res = client
            .post(format!("{url}/tips"))
            .header("Authorization", &api_key)
            .body(tips_query.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);

        let tips_resp =
            crate::api::pb::TipsResponse::decode(res.bytes().await.unwrap()).unwrap();
        assert_eq!(tips_resp.tips.len(), 3);

        let topics: Vec<&str> = tips_resp.tips.iter().map(|t| t.topic.as_str()).collect();
        assert!(topics.contains(&"rust"));
        assert!(topics.contains(&"python"));
        assert!(topics.contains(&"go"));
    }
}


