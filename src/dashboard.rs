use crate::{api, AppState};
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Json,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::sync::Arc;

pub async fn index(State(state): State<Arc<AppState>>) -> Response {
    match fs::read_to_string(state.template_dir.join("admin.html")) {
        Ok(html) => Html(html).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Admin template missing").into_response(),
    }
}

pub async fn app_index(State(state): State<Arc<AppState>>) -> Response {
    match fs::read_to_string(state.template_dir.join("app.html")) {
        Ok(html) => Html(html).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Client template missing").into_response(),
    }
}

#[derive(Serialize)]
pub struct SettingsRes {
    model: String,
    compress_model: String,
    template: String,
    api_key: String,
    base_url: String,
    compress_base_url: String,
    reasoning_effort: String,
    compress_reasoning_effort: String,
    color_scheme: String,
}

pub async fn get_settings(State(state): State<Arc<AppState>>) -> Json<SettingsRes> {
    let settings_str = fs::read_to_string(&state.settings_path).unwrap_or_default();
    let settings: serde_yaml::Value = serde_yaml::from_str(&settings_str).unwrap_or_default();

    let model = settings
        .get("llm_model")
        .and_then(|v| v.as_str())
        .unwrap_or("google/gemini-3.1-flash")
        .to_string();
    let template = settings
        .get("prompt_template")
        .and_then(|v| v.as_str())
        .unwrap_or("Give a smart tip about {topic}.")
        .to_string();
    let api_key = settings
        .get("llm_api_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let base_url = settings
        .get("llm_base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://openrouter.ai/api/v1")
        .to_string();
    let compress_model = settings
        .get("llm_compress_model")
        .and_then(|v| v.as_str())
        .unwrap_or("google/gemini-3.1-flash-lite-preview")
        .to_string();
    let compress_base_url = settings
        .get("llm_compress_base_url")
        .and_then(|v| v.as_str())
        .unwrap_or(&base_url)
        .to_string();
    let reasoning_effort = settings
        .get("llm_reasoning_effort")
        .and_then(|v| v.as_str())
        .unwrap_or("none")
        .to_string();
    let compress_reasoning_effort = settings
        .get("llm_compress_reasoning_effort")
        .and_then(|v| v.as_str())
        .unwrap_or("none")
        .to_string();
    let color_scheme = settings
        .get("color_scheme")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();

    Json(SettingsRes {
        model,
        compress_model,
        template,
        api_key,
        base_url,
        compress_base_url,
        reasoning_effort,
        compress_reasoning_effort,
        color_scheme,
    })
}

#[derive(Deserialize)]
pub struct UpdateSettingsReq {
    model: Option<String>,
    compress_model: Option<String>,
    template: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    compress_base_url: Option<String>,
    reasoning_effort: Option<String>,
    compress_reasoning_effort: Option<String>,
    color_scheme: Option<String>,
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateSettingsReq>,
) -> Json<()> {
    let settings_str = fs::read_to_string(&state.settings_path).unwrap_or_default();
    let mut settings: serde_yaml::Value = serde_yaml::from_str(&settings_str)
        .unwrap_or(serde_yaml::Value::Mapping(Default::default()));
    if !settings.is_mapping() {
        settings = serde_yaml::Value::Mapping(Default::default());
    }

    if let serde_yaml::Value::Mapping(ref mut map) = settings {
        if let Some(model) = req.model {
            map.insert(
                serde_yaml::Value::String("llm_model".to_string()),
                serde_yaml::Value::String(model),
            );
        }
        if let Some(compress_model) = req.compress_model {
            map.insert(
                serde_yaml::Value::String("llm_compress_model".to_string()),
                serde_yaml::Value::String(compress_model),
            );
        }
        if let Some(template) = req.template {
            map.insert(
                serde_yaml::Value::String("prompt_template".to_string()),
                serde_yaml::Value::String(template),
            );
        }
        if let Some(api_key) = req.api_key {
            map.insert(
                serde_yaml::Value::String("llm_api_key".to_string()),
                serde_yaml::Value::String(api_key),
            );
        }
        if let Some(base_url) = req.base_url {
            map.insert(
                serde_yaml::Value::String("llm_base_url".to_string()),
                serde_yaml::Value::String(base_url),
            );
        }
        if let Some(compress_base_url) = req.compress_base_url {
            map.insert(
                serde_yaml::Value::String("llm_compress_base_url".to_string()),
                serde_yaml::Value::String(compress_base_url),
            );
        }
        if let Some(reasoning_effort) = req.reasoning_effort {
            map.insert(
                serde_yaml::Value::String("llm_reasoning_effort".to_string()),
                serde_yaml::Value::String(reasoning_effort),
            );
        }
        if let Some(compress_reasoning_effort) = req.compress_reasoning_effort {
            map.insert(
                serde_yaml::Value::String("llm_compress_reasoning_effort".to_string()),
                serde_yaml::Value::String(compress_reasoning_effort),
            );
        }
        if let Some(color_scheme) = req.color_scheme {
            map.insert(
                serde_yaml::Value::String("color_scheme".to_string()),
                serde_yaml::Value::String(color_scheme),
            );
        }
    }

    let out_str = serde_yaml::to_string(&settings).unwrap();
    fs::write(&state.settings_path, out_str).unwrap();

    Json(())
}

#[derive(Deserialize)]
pub struct CreateKeyReq {
    pub client_name: Option<String>,
}

pub async fn create_api_key(
    State(state): State<Arc<AppState>>,
    req: Option<Json<CreateKeyReq>>,
) -> Json<String> {
    let raw_key: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let api_key = format!("sk_live_{}", raw_key);

    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    let client_name = req
        .and_then(|Json(r)| r.client_name)
        .unwrap_or_else(|| "default_client".to_string());

    let _ = sqlx::query("INSERT INTO api_keys (key_hash, client_name) VALUES (?, ?)")
        .bind(key_hash)
        .bind(client_name)
        .execute(&state.db)
        .await;

    Json(api_key)
}

#[derive(Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub client_name: String,
    pub created_at: String,
}

pub async fn list_api_keys(State(state): State<Arc<AppState>>) -> Json<Vec<ApiKeyInfo>> {
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, client_name, COALESCE(CAST(created_at AS TEXT), '') FROM api_keys ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let keys = rows
        .into_iter()
        .map(|row| ApiKeyInfo {
            id: row.0,
            client_name: row.1,
            created_at: row.2,
        })
        .collect();

    Json(keys)
}

#[derive(Deserialize)]
pub struct DeleteKeyReq {
    pub id: i64,
}

pub async fn delete_api_key(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeleteKeyReq>,
) -> StatusCode {
    let result = sqlx::query("DELETE FROM api_keys WHERE id = ?")
        .bind(req.id)
        .execute(&state.db)
        .await;

    if result.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Deserialize)]
pub struct DeleteTipcardReq {
    pub id: i64,
}

pub async fn delete_tipcard(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeleteTipcardReq>,
) -> StatusCode {
    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    if sqlx::query("DELETE FROM review_states WHERE card_id = ?")
        .bind(req.id)
        .execute(&mut *tx)
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    if sqlx::query("DELETE FROM tipcards WHERE id = ?")
        .bind(req.id)
        .execute(&mut *tx)
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    match tx.commit().await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Serialize)]
pub struct TopicInfo {
    pub id: i64,
    pub name: String,
}

pub async fn list_topics(State(state): State<Arc<AppState>>) -> Json<Vec<TopicInfo>> {
    let rows = sqlx::query_as::<_, (i64, String)>("SELECT id, name FROM topics ORDER BY name ASC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let topics = rows
        .into_iter()
        .map(|r| TopicInfo { id: r.0, name: r.1 })
        .collect();
    Json(topics)
}

#[derive(Serialize)]
pub struct TopicClassInfo {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
}

pub async fn list_topic_classes(State(state): State<Arc<AppState>>) -> Json<Vec<TopicClassInfo>> {
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, name, tipcard_type FROM topic_classes ORDER BY name ASC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    Json(
        rows.into_iter()
            .map(|r| TopicClassInfo {
                id: r.0,
                name: r.1,
                tipcard_type: r.2,
            })
            .collect(),
    )
}

#[derive(Serialize)]
pub struct AppSummary {
    pub topics: i64,
    pub total_cards: i64,
    pub due_cards: i64,
    pub active_cards: i64,
}

pub async fn app_summary(State(state): State<Arc<AppState>>) -> Json<AppSummary> {
    let now = chrono::Utc::now();
    let topics = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM topics")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let total_cards = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tipcards")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let due_cards = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM review_states
         WHERE status = 'active' AND next_review_at <= ?",
    )
    .bind(now)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    let active_cards =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM review_states WHERE status = 'active'")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    Json(AppSummary {
        topics,
        total_cards,
        due_cards,
        active_cards,
    })
}

#[derive(Serialize)]
pub struct AppTopicInfo {
    pub id: i64,
    pub name: String,
    pub class_name: String,
    pub tipcard_type: String,
    pub total_cards: i64,
    pub due_cards: i64,
    pub completed_cards: i64,
}

pub async fn app_topics(State(state): State<Arc<AppState>>) -> Json<Vec<AppTopicInfo>> {
    let now = chrono::Utc::now();
    let rows = sqlx::query_as::<_, (i64, String, Option<String>, Option<String>, i64, i64, i64)>(
        "SELECT top.id,
                top.name,
                tc.name AS class_name,
                tc.tipcard_type,
                COUNT(t.id) AS total_cards,
                SUM(CASE WHEN r.status = 'active' AND r.next_review_at <= ? THEN 1 ELSE 0 END) AS due_cards,
                SUM(CASE WHEN r.status != 'active' THEN 1 ELSE 0 END) AS completed_cards
         FROM topics top
         LEFT JOIN topic_classes tc ON top.class_id = tc.id
         LEFT JOIN tipcards t ON t.topic_id = top.id
         LEFT JOIN review_states r ON r.card_id = t.id
         GROUP BY top.id, top.name, tc.name, tc.tipcard_type
         ORDER BY due_cards DESC, top.name ASC",
    )
    .bind(now)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    Json(
        rows.into_iter()
            .map(|r| AppTopicInfo {
                id: r.0,
                name: r.1,
                class_name: r.2.unwrap_or_else(|| "default".to_string()),
                tipcard_type: r.3.unwrap_or_else(|| "srs_tip".to_string()),
                total_cards: r.4,
                due_cards: r.5,
                completed_cards: r.6,
            })
            .collect(),
    )
}

#[derive(Serialize, Deserialize)]
pub struct TipcardInfo {
    pub id: i64,
    pub topic_name: String,
    pub full_content: String,
    pub compressed_content: String,
    pub created_at: String,
    pub tipcard_type: String,
    pub topic_class: String,
    pub status: String,
    pub next_review_at: String,
    pub repeat_count: u32,
}

pub async fn list_tipcards(State(state): State<Arc<AppState>>) -> Json<Vec<TipcardInfo>> {
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        ),
    >(
        "SELECT t.id,
                top.name AS topic_name,
                t.full_content,
                t.compressed_content,
                COALESCE(CAST(t.created_at AS TEXT), '') AS created_at,
                t.tipcard_type,
                COALESCE(tc.name, 'default') AS topic_class,
                COALESCE(r.status, 'active') AS status,
                COALESCE(CAST(r.next_review_at AS TEXT), '') AS next_review_at,
                COALESCE(r.state_data, '') AS state_data
         FROM tipcards t
         JOIN topics top ON t.topic_id = top.id
         LEFT JOIN topic_classes tc ON top.class_id = tc.id
         LEFT JOIN review_states r ON r.card_id = t.id
         ORDER BY t.created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let cards = rows
        .into_iter()
        .map(|r| TipcardInfo {
            id: r.0,
            topic_name: r.1,
            full_content: r.2,
            compressed_content: r.3,
            created_at: r.4,
            tipcard_type: r.5,
            topic_class: r.6,
            status: r.7,
            next_review_at: r.8,
            repeat_count: serde_json::from_str::<serde_json::Value>(&r.9)
                .ok()
                .and_then(|value| value.get("repeats").and_then(|repeats| repeats.as_u64()))
                .unwrap_or(0) as u32,
        })
        .collect();

    Json(cards)
}

pub async fn app_tips(
    State(state): State<Arc<AppState>>,
    Json(req): Json<api::TipsJsonRequest>,
) -> Result<Json<Vec<api::TipCardJson>>, (StatusCode, String)> {
    api::build_tips(&state, req).await.map(Json)
}

pub async fn app_review(
    State(state): State<Arc<AppState>>,
    Json(req): Json<api::ReviewJsonRequest>,
) -> Result<Json<()>, (StatusCode, String)> {
    let grade = req.grade.unwrap_or(3).min(5);
    let action = req.action.unwrap_or_default();
    api::apply_review(&state, req.card_id, grade, &action).await?;
    Ok(Json(()))
}
