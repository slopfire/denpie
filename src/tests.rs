#[cfg(test)]
mod tests {
    use crate::{apply_schema_migrations, build_app, AppState};
    use prost::Message;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::fs;
    use tower_sessions::{MemoryStore, SessionManagerLayer};

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
        apply_schema_migrations(&pool).await.unwrap();
        pool
    }

    /// Write isolated test settings and spin up a real server on an ephemeral port.
    /// Returns (base_url, reqwest::Client with cookie jar).
    async fn spawn_test_server() -> (String, reqwest::Client) {
        let test_token = "test_admin_token_xyz";
        let settings_path = unique_settings_path();
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("admin_token".into()),
            serde_yaml::Value::String(test_token.into()),
        );
        let settings_val = serde_yaml::Value::Mapping(map);
        fs::write(
            &settings_path,
            serde_yaml::to_string(&settings_val).unwrap(),
        )
        .await
        .unwrap();

        let db = setup_db().await;
        let state = Arc::new(AppState { db, settings_path });
        let session_store = MemoryStore::default();
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

    fn unique_settings_path() -> PathBuf {
        let suffix: u64 = rand::random();
        std::env::temp_dir().join(format!("dailytipdraft-test-settings-{suffix}.yaml"))
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
        let settings = client
            .get(format!("{url}/admin/settings"))
            .send()
            .await
            .unwrap();
        assert_eq!(settings.status(), reqwest::StatusCode::UNAUTHORIZED);

        let keys = client
            .get(format!("{url}/admin/keys"))
            .send()
            .await
            .unwrap();
        assert_eq!(keys.status(), reqwest::StatusCode::UNAUTHORIZED);

        let create = client
            .post(format!("{url}/admin/keys"))
            .json(&serde_json::json!({ "client_name": "nope" }))
            .send()
            .await
            .unwrap();
        assert_eq!(create.status(), reqwest::StatusCode::UNAUTHORIZED);

        let app_summary = client
            .get(format!("{url}/app/summary"))
            .send()
            .await
            .unwrap();
        assert_eq!(app_summary.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    // ──────────────────────────────────────────────
    //  Admin Dashboard HTML
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_root_app_page_serves_html() {
        let (url, client) = spawn_test_server().await;
        let res = client.get(format!("{url}/")).send().await.unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let body = res.text().await.unwrap();
        assert!(
            body.contains("MindLift SRS"),
            "Root app should contain the app title"
        );
        assert!(
            body.contains("admin-token"),
            "Root app should contain the login input"
        );
        assert!(
            body.contains("theme-select"),
            "Root app should contain color scheme controls"
        );
        assert!(
            body.contains("flow-list-btn"),
            "Root app should contain the flow layout toggle"
        );
        assert!(
            body.contains("renderMarkdown"),
            "Root app should render tipcard markdown"
        );
        assert!(
            body.contains("markdown-content"),
            "Root app should include markdown tipcard styles"
        );
        assert!(
            !body.contains("tips-class"),
            "Root app should not expose a card class text field"
        );
        assert!(
            !body.contains("<option value=\"srs_tip\">"),
            "Root app should not expose SRS as a card class"
        );
    }

    #[tokio::test]
    async fn test_admin_page_serves_html() {
        let (url, client) = spawn_test_server().await;
        let res = client.get(format!("{url}/admin")).send().await.unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let body = res.text().await.unwrap();
        assert!(
            body.contains("DAILY"),
            "Admin page should contain the app title"
        );
        assert!(
            body.contains("TIP"),
            "Admin page should contain the app title part 2"
        );
        assert!(
            body.contains("adminTokenInput"),
            "Admin page should contain the login input"
        );
        assert!(
            body.contains("renderMarkdown"),
            "Admin page should render tipcard markdown"
        );
        assert!(
            body.contains("markdown-content"),
            "Admin page should include markdown tipcard styles"
        );
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
        let res = client
            .get(format!("{url}/admin/settings"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let data: serde_json::Value = res.json().await.unwrap();
        assert!(data["model"].as_str().is_some());
        assert!(data["template"].as_str().is_some());
        assert_eq!(data["color_scheme"], "default");

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
        assert_eq!(
            update_status,
            reqwest::StatusCode::OK,
            "Settings update failed: body={}",
            update_body
        );

        // GET again to verify persistence
        let res = client
            .get(format!("{url}/admin/settings"))
            .send()
            .await
            .unwrap();
        let data: serde_json::Value = res.json().await.unwrap();
        assert_eq!(data["model"], "google/gemini-2.5-pro");
        assert_eq!(data["template"], "Tell me a fun fact about {topic}.");

        let theme_update_res = client
            .post(format!("{url}/admin/settings"))
            .json(&serde_json::json!({
                "color_scheme": "dracula"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(theme_update_res.status(), reqwest::StatusCode::OK);

        let res = client
            .get(format!("{url}/admin/settings"))
            .send()
            .await
            .unwrap();
        let data: serde_json::Value = res.json().await.unwrap();
        assert_eq!(data["model"], "google/gemini-2.5-pro");
        assert_eq!(data["color_scheme"], "dracula");
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
        assert!(
            key1.starts_with("sk_live_"),
            "Key should have sk_live_ prefix"
        );

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

    #[tokio::test]
    async fn test_browser_app_tip_review_flow() {
        let (url, client) = spawn_test_server().await;
        client
            .post(format!("{url}/auth/login"))
            .json(&serde_json::json!({ "admin_token": "test_admin_token_xyz" }))
            .send()
            .await
            .unwrap();

        let tips: serde_json::Value = client
            .post(format!("{url}/app/tips"))
            .json(&serde_json::json!({
                "topics": "rust",
                "topic_class": "casual",
                "tipcard_type": "casual_tip",
                "count": 1
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let first = tips.as_array().unwrap().first().unwrap();
        assert_eq!(first["topic"], "rust");
        assert_eq!(first["topic_class"], "casual");
        assert_eq!(first["tipcard_type"], "casual_tip");

        let review = client
            .post(format!("{url}/app/review"))
            .json(&serde_json::json!({
                "card_id": first["id"],
                "action": "acknowledge"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(review.status(), reqwest::StatusCode::OK);

        let summary: serde_json::Value = client
            .get(format!("{url}/app/summary"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(summary["topics"], 1);
        assert_eq!(summary["total_cards"], 1);
    }

    #[tokio::test]
    async fn test_browser_app_can_delete_tipcard() {
        let (url, client) = spawn_test_server().await;
        client
            .post(format!("{url}/auth/login"))
            .json(&serde_json::json!({ "admin_token": "test_admin_token_xyz" }))
            .send()
            .await
            .unwrap();

        let tips: serde_json::Value = client
            .post(format!("{url}/app/tips"))
            .json(&serde_json::json!({
                "topics": "rust",
                "topic_class": "repeatable",
                "tipcard_type": "repeatable_tip",
                "count": 1
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let card_id = tips.as_array().unwrap().first().unwrap()["id"]
            .as_i64()
            .unwrap();

        let delete = client
            .delete(format!("{url}/admin/tipcards"))
            .json(&serde_json::json!({ "id": card_id }))
            .send()
            .await
            .unwrap();
        assert_eq!(delete.status(), reqwest::StatusCode::OK);

        let cards: Vec<crate::dashboard::TipcardInfo> = client
            .get(format!("{url}/admin/tipcards"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(cards.iter().all(|card| card.id != card_id));

        let review = client
            .post(format!("{url}/app/review"))
            .json(&serde_json::json!({ "card_id": card_id, "action": "dismiss" }))
            .send()
            .await
            .unwrap();
        assert_eq!(review.status(), reqwest::StatusCode::NOT_FOUND);
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
            topic_class: "".into(),
            tipcard_type: "".into(),
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
            topic_class: "".into(),
            tipcard_type: "".into(),
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
            topic_class: "".into(),
            tipcard_type: "".into(),
        };
        let res = client
            .post(format!("{url}/tips"))
            .header("Authorization", &api_key)
            .body(tips_query.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);

        let tips_resp = crate::api::pb::TipsResponse::decode(res.bytes().await.unwrap()).unwrap();
        assert_eq!(tips_resp.tips.len(), 1);
        let card_id = tips_resp.tips[0].id;
        assert!(!tips_resp.tips[0].full_content.is_empty());
        assert!(!tips_resp.tips[0].compressed_content.is_empty());
        assert_eq!(tips_resp.tips[0].topic, "rust");

        // Review that card — should succeed
        let review = crate::api::pb::ReviewPayload {
            card_id,
            grade: 4,
            action: "".into(),
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
            action: "".into(),
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
            topic_class: "".into(),
            tipcard_type: "".into(),
        };
        let res = client
            .post(format!("{url}/tips"))
            .header("Authorization", &api_key)
            .body(tips_query.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);

        let tips_resp = crate::api::pb::TipsResponse::decode(res.bytes().await.unwrap()).unwrap();
        assert_eq!(tips_resp.tips.len(), 3);

        let topics: Vec<&str> = tips_resp.tips.iter().map(|t| t.topic.as_str()).collect();
        assert!(topics.contains(&"rust"));
        assert!(topics.contains(&"python"));
        assert!(topics.contains(&"go"));
    }

    #[tokio::test]
    async fn test_repeatable_tipcards_can_dismiss_and_get_new_card() {
        let (url, client) = spawn_test_server().await;

        client
            .post(format!("{url}/auth/login"))
            .json(&serde_json::json!({ "admin_token": "test_admin_token_xyz" }))
            .send()
            .await
            .unwrap();
        let api_key: String = client
            .post(format!("{url}/admin/keys"))
            .json(&serde_json::json!({ "client_name": "repeatable_flow" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let tips_query = crate::api::pb::TipsQuery {
            count: 1,
            topics: "spanish".into(),
            topic_class: "re:word".into(),
            tipcard_type: "repeatable_tip".into(),
        };
        let res = client
            .post(format!("{url}/tips"))
            .header("Authorization", &api_key)
            .body(tips_query.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let first_resp = crate::api::pb::TipsResponse::decode(res.bytes().await.unwrap()).unwrap();
        assert_eq!(first_resp.tips.len(), 1);
        assert_eq!(first_resp.tips[0].topic_class, "re:word");
        assert_eq!(first_resp.tips[0].tipcard_type, "repeatable_tip");
        let first_id = first_resp.tips[0].id;

        let dismiss = crate::api::pb::ReviewPayload {
            card_id: first_id,
            grade: 0,
            action: "dismiss".into(),
        };
        let res = client
            .post(format!("{url}/review"))
            .header("Authorization", &api_key)
            .body(dismiss.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);

        let res = client
            .post(format!("{url}/tips"))
            .header("Authorization", &api_key)
            .body(tips_query.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let second_resp = crate::api::pb::TipsResponse::decode(res.bytes().await.unwrap()).unwrap();
        assert_eq!(second_resp.tips.len(), 1);
        assert_ne!(second_resp.tips[0].id, first_id);
    }

    #[tokio::test]
    async fn test_casual_tipcards_can_dismiss_or_acknowledge_and_get_new_card() {
        let (url, client) = spawn_test_server().await;

        client
            .post(format!("{url}/auth/login"))
            .json(&serde_json::json!({ "admin_token": "test_admin_token_xyz" }))
            .send()
            .await
            .unwrap();
        let api_key: String = client
            .post(format!("{url}/admin/keys"))
            .json(&serde_json::json!({ "client_name": "casual_flow" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let tips_query = crate::api::pb::TipsQuery {
            count: 1,
            topics: "rust".into(),
            topic_class: "casual".into(),
            tipcard_type: "casual_tip".into(),
        };
        let res = client
            .post(format!("{url}/tips"))
            .header("Authorization", &api_key)
            .body(tips_query.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let first_resp = crate::api::pb::TipsResponse::decode(res.bytes().await.unwrap()).unwrap();
        assert_eq!(first_resp.tips.len(), 1);
        assert_eq!(first_resp.tips[0].topic_class, "casual");
        assert_eq!(first_resp.tips[0].tipcard_type, "casual_tip");
        let first_id = first_resp.tips[0].id;

        let dismiss = crate::api::pb::ReviewPayload {
            card_id: first_id,
            grade: 0,
            action: "dismiss".into(),
        };
        let res = client
            .post(format!("{url}/review"))
            .header("Authorization", &api_key)
            .body(dismiss.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);

        let res = client
            .post(format!("{url}/tips"))
            .header("Authorization", &api_key)
            .body(tips_query.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let second_resp = crate::api::pb::TipsResponse::decode(res.bytes().await.unwrap()).unwrap();
        assert_eq!(second_resp.tips.len(), 1);
        assert_ne!(second_resp.tips[0].id, first_id);
        let second_id = second_resp.tips[0].id;

        let acknowledge = crate::api::pb::ReviewPayload {
            card_id: second_id,
            grade: 5,
            action: "acknowledge".into(),
        };
        let res = client
            .post(format!("{url}/review"))
            .header("Authorization", &api_key)
            .body(acknowledge.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);

        let res = client
            .post(format!("{url}/tips"))
            .header("Authorization", &api_key)
            .body(tips_query.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let third_resp = crate::api::pb::TipsResponse::decode(res.bytes().await.unwrap()).unwrap();
        assert_eq!(third_resp.tips.len(), 1);
        assert_ne!(third_resp.tips[0].id, second_id);
    }
}
