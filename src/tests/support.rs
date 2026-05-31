use crate::{AppState, apply_schema_migrations, build_app};
use prost::Message;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};
use tokio::fs;
use tower_sessions::{MemoryStore, SessionManagerLayer};

pub(super) const TEST_USER_ID: &str = "usr_test_admin";
static TEST_FRONTEND_DIST: OnceLock<PathBuf> = OnceLock::new();

pub(super) async fn setup_db() -> SqlitePool {
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
    sqlx::query("INSERT OR IGNORE INTO users (id, username, password_hash, role) VALUES (?, ?, ?, ?)")
        .bind(TEST_USER_ID)
        .bind("admin")
        .bind("$argon2id$v=19$m=65536,t=3,p=4$vYeSOJhiAbCZq6BNzhy5QA$GZ91eZlkhpmtBYSas36hb50QqbHOL5FofnhBDFBklHM")
        .bind("admin")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

/// Write isolated test settings and spin up a real server on an ephemeral port.
/// Returns (base_url, reqwest::Client with cookie jar).
pub(super) async fn spawn_test_server() -> (String, reqwest::Client) {
    ensure_test_frontend_dist();
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
    let state = Arc::new(make_state(db, settings_path));
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store).with_secure(false);
    let app = build_app(state, session_layer);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();

    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .json(&serde_json::json!({
            "admin_token": test_token,
            "username": "admin",
            "password": "test_password_123"
        }))
        .send()
        .await
        .unwrap();

    if setup.status() == reqwest::StatusCode::CONFLICT {
        let login = client
            .post(format!("{base_url}/auth/login"))
            .json(&serde_json::json!({
                "username": "admin",
                "password": "test_password_123"
            }))
            .send()
            .await
            .unwrap();
        assert!(
            login.status().is_success(),
            "login status after conflict {}",
            login.status()
        );
    } else {
        assert!(
            setup.status().is_success(),
            "setup status {}",
            setup.status()
        );
    }

    (base_url, client)
}

fn ensure_test_frontend_dist() {
    let frontend_dist = TEST_FRONTEND_DIST.get_or_init(|| {
        let path =
            std::env::temp_dir().join(format!("denpie-test-frontend-dist-{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create test frontend dist");
        std::fs::write(
            path.join("index.html"),
            r#"<!doctype html>
<html>
  <head>
    <title>Denpie</title>
    <link rel="modulepreload" href="/frontend-test.js">
  </head>
  <body>
    <script type="module" src="/frontend-test.js"></script>
    <link rel="preload" href="/frontend-test_bg.wasm" as="fetch" type="application/wasm">
  </body>
</html>
"#,
        )
        .expect("write test frontend index");
        path
    });

    // Tests build the app before CI has produced frontend/dist. Point every test
    // server at a tiny stable fixture so the SPA fallback is deterministic.
    unsafe {
        std::env::set_var("DENPIE_FRONTEND_DIST", frontend_dist);
    }
}

pub(super) fn unique_settings_path() -> PathBuf {
    let suffix: u64 = rand::random();
    std::env::temp_dir().join(format!("denpie-test-settings-{suffix}.yaml"))
}

pub(super) fn make_state(db: SqlitePool, settings_path: PathBuf) -> AppState {
    let settings_store = crate::config::SettingsStore::new(settings_path.clone());
    let image_dir = settings_path.with_extension("images");
    let rp_id = "localhost";
    let rp_origin = url::Url::parse("http://localhost:3017").unwrap();
    let webauthn = Arc::new(
        webauthn_rs::WebauthnBuilder::new(rp_id, &rp_origin)
            .unwrap()
            .build()
            .unwrap(),
    );
    AppState {
        api_keys: crate::services::api_keys::ApiKeyService::new(db.clone()),
        settings: crate::services::settings::SettingsService::new(settings_store),
        reviews: crate::services::review::ReviewService::new(db.clone()),
        db,
        image_dir,
        settings_path,
        webauthn,
    }
}

pub(super) async fn bootstrap_api_key(
    url: &str,
    client: &reqwest::Client,
    client_name: &str,
) -> String {
    let request = crate::api::pb::ApiRequest {
        auth: "".into(),
        op: Some(crate::api::pb::api_request::Op::BootstrapApiKey(
            crate::api::pb::BootstrapApiKeyRequest {
                admin_token: "test_admin_token_xyz".into(),
                client_name: client_name.into(),
            },
        )),
    };
    let response = client
        .post(format!("{url}/api"))
        .header("Content-Type", "application/x-protobuf")
        .body(request.encode_to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let response = crate::api::pb::ApiResponse::decode(response.bytes().await.unwrap()).unwrap();
    match response.result.unwrap() {
        crate::api::pb::api_response::Result::ApiKeyCreated(created) => created.api_key,
        other => panic!("unexpected response: {:?}", other),
    }
}

pub(super) async fn post_api(
    url: &str,
    client: &reqwest::Client,
    request: crate::api::pb::ApiRequest,
) -> reqwest::Response {
    client
        .post(format!("{url}/api"))
        .header("Content-Type", "application/x-protobuf")
        .body(request.encode_to_vec())
        .send()
        .await
        .unwrap()
}
