use axum::{
    body::Bytes,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use prost::Message;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{QueryBuilder, Sqlite};
use std::fs;
use std::sync::Arc;

use crate::{
    context, llm,
    srs::{self, Algorithm, SrsState},
    AppState,
};

pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/dailytip.rs"));
}

fn protobuf_response<T: Message>(msg: &T) -> Response {
    let mut buf = bytes::BytesMut::with_capacity(msg.encoded_len());
    msg.encode(&mut buf).unwrap();
    (
        [(header::CONTENT_TYPE, "application/x-protobuf")],
        buf.freeze(),
    )
        .into_response()
}

fn empty_response() -> pb::ApiResponse {
    pb::ApiResponse {
        result: Some(pb::api_response::Result::Ok(pb::Empty {})),
    }
}

fn hash_api_key(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    hex::encode(hasher.finalize())
}

async fn require_api_key(state: &AppState, api_key: &str) -> Result<(), (StatusCode, String)> {
    if api_key.trim().is_empty() {
        return Err((StatusCode::UNAUTHORIZED, "Missing API key".to_string()));
    }

    let exists: Option<String> =
        sqlx::query_scalar("SELECT client_name FROM api_keys WHERE key_hash = ?")
            .bind(hash_api_key(api_key))
            .fetch_optional(&state.db)
            .await
            .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if exists.is_some() {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()))
    }
}

async fn create_raw_api_key(
    state: &AppState,
    client_name: Option<String>,
) -> Result<String, (StatusCode, String)> {
    let raw_key: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let api_key = format!("sk_live_{}", raw_key);
    let client_name = client_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "default_client".to_string());

    sqlx::query("INSERT INTO api_keys (key_hash, client_name) VALUES (?, ?)")
        .bind(hash_api_key(&api_key))
        .bind(client_name)
        .execute(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(api_key)
}

fn read_settings_value(state: &AppState) -> serde_yaml::Value {
    let settings_str = fs::read_to_string(&state.settings_path).unwrap_or_default();
    let settings: serde_yaml::Value = serde_yaml::from_str(&settings_str)
        .unwrap_or(serde_yaml::Value::Mapping(Default::default()));
    if settings.is_mapping() {
        settings
    } else {
        serde_yaml::Value::Mapping(Default::default())
    }
}

fn current_settings(state: &AppState) -> pb::Settings {
    let settings = read_settings_value(state);
    let base_url = settings
        .get("llm_base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://openrouter.ai/api/v1")
        .to_string();

    pb::Settings {
        model: settings
            .get("llm_model")
            .and_then(|v| v.as_str())
            .unwrap_or("google/gemini-3.1-flash")
            .to_string(),
        compress_model: settings
            .get("llm_compress_model")
            .and_then(|v| v.as_str())
            .unwrap_or("google/gemini-3.1-flash-lite-preview")
            .to_string(),
        template: settings
            .get("prompt_template")
            .and_then(|v| v.as_str())
            .unwrap_or("Give a smart tip about {topic}.")
            .to_string(),
        api_key: settings
            .get("llm_api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        base_url: base_url.clone(),
        compress_base_url: settings
            .get("llm_compress_base_url")
            .and_then(|v| v.as_str())
            .unwrap_or(&base_url)
            .to_string(),
        reasoning_effort: settings
            .get("llm_reasoning_effort")
            .and_then(|v| v.as_str())
            .unwrap_or("none")
            .to_string(),
        compress_reasoning_effort: settings
            .get("llm_compress_reasoning_effort")
            .and_then(|v| v.as_str())
            .unwrap_or("none")
            .to_string(),
        color_scheme: settings
            .get("color_scheme")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string(),
        autoupdate_enabled: settings
            .get("autoupdate_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        autoupdate_repo: settings
            .get("autoupdate_repo")
            .and_then(|v| v.as_str())
            .unwrap_or("slopfire/dailytipdraft")
            .to_string(),
        autoupdate_branch: settings
            .get("autoupdate_branch")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string(),
        autoupdate_check_interval_secs: settings
            .get("autoupdate_check_interval_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(3600),
        autoupdate_command: settings
            .get("autoupdate_command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        autoupdate_last_seen_sha: settings
            .get("autoupdate_last_seen_sha")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

fn put_string_setting(map: &mut serde_yaml::Mapping, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(
            serde_yaml::Value::String(key.to_string()),
            serde_yaml::Value::String(value),
        );
    }
}

fn put_bool_setting(map: &mut serde_yaml::Mapping, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        map.insert(
            serde_yaml::Value::String(key.to_string()),
            serde_yaml::Value::Bool(value),
        );
    }
}

fn put_u64_setting(map: &mut serde_yaml::Mapping, key: &str, value: Option<u64>) {
    if let Some(value) = value {
        map.insert(
            serde_yaml::Value::String(key.to_string()),
            serde_yaml::Value::Number(value.into()),
        );
    }
}

fn update_settings_file(
    state: &AppState,
    req: pb::UpdateSettingsRequest,
) -> Result<(), (StatusCode, String)> {
    let mut settings = read_settings_value(state);
    if let serde_yaml::Value::Mapping(ref mut map) = settings {
        put_string_setting(map, "llm_model", req.model);
        put_string_setting(map, "llm_compress_model", req.compress_model);
        put_string_setting(map, "prompt_template", req.template);
        put_string_setting(map, "llm_api_key", req.api_key);
        put_string_setting(map, "llm_base_url", req.base_url);
        put_string_setting(map, "llm_compress_base_url", req.compress_base_url);
        put_string_setting(map, "llm_reasoning_effort", req.reasoning_effort);
        put_string_setting(
            map,
            "llm_compress_reasoning_effort",
            req.compress_reasoning_effort,
        );
        put_string_setting(map, "color_scheme", req.color_scheme);
        put_bool_setting(map, "autoupdate_enabled", req.autoupdate_enabled);
        put_string_setting(map, "autoupdate_repo", req.autoupdate_repo);
        put_string_setting(map, "autoupdate_branch", req.autoupdate_branch);
        put_u64_setting(
            map,
            "autoupdate_check_interval_secs",
            req.autoupdate_check_interval_secs,
        );
        put_string_setting(map, "autoupdate_command", req.autoupdate_command);
    }

    let out_str = serde_yaml::to_string(&settings)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    fs::write(&state.settings_path, out_str)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

#[derive(Serialize, Deserialize, Default)]
struct RepeatableState {
    repeats: u32,
}

#[derive(Clone)]
struct TopicClassInfo {
    id: i64,
    name: String,
    tipcard_type: String,
}

#[derive(Clone)]
struct TopicInfo {
    id: i64,
    prompt_template: Option<String>,
}

#[derive(Deserialize)]
pub struct TipsJsonRequest {
    pub count: Option<u32>,
    pub topics: String,
    pub topic_class: Option<String>,
    pub tipcard_type: Option<String>,
    pub exclude_card_ids: Option<Vec<i64>>,
}

#[derive(Clone, Serialize)]
pub struct TipCardJson {
    pub id: i64,
    pub topic: String,
    pub full_content: String,
    pub compressed_content: String,
    pub topic_class: String,
    pub tipcard_type: String,
}

#[derive(Deserialize)]
pub struct ReviewJsonRequest {
    pub card_id: i64,
    pub grade: Option<u8>,
    pub action: Option<String>,
}

async fn list_api_keys_pb(state: &AppState) -> Result<pb::ApiKeys, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, client_name, COALESCE(CAST(created_at AS TEXT), '') FROM api_keys ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(pb::ApiKeys {
        keys: rows
            .into_iter()
            .map(|row| pb::ApiKeyInfo {
                id: row.0,
                client_name: row.1,
                created_at: row.2,
            })
            .collect(),
    })
}

async fn delete_api_key_by_id(state: &AppState, id: i64) -> Result<(), (StatusCode, String)> {
    sqlx::query("DELETE FROM api_keys WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

async fn list_admin_topics_pb(state: &AppState) -> Result<pb::AdminTopics, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, (i64, String, Option<String>)>(
        "SELECT id, name, prompt_template FROM topics ORDER BY name ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(pb::AdminTopics {
        topics: rows
            .into_iter()
            .map(|row| pb::AdminTopic {
                id: row.0,
                name: row.1,
                prompt_template: row.2.unwrap_or_default(),
            })
            .collect(),
    })
}

async fn delete_tipcard_by_id(state: &AppState, id: i64) -> Result<(), (StatusCode, String)> {
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    sqlx::query("DELETE FROM review_states WHERE card_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    sqlx::query("DELETE FROM tipcards WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tx.commit()
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

async fn list_tipcards_pb(state: &AppState) -> Result<pb::Tipcards, (StatusCode, String)> {
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
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(pb::Tipcards {
        cards: rows
            .into_iter()
            .map(|row| pb::TipcardInfo {
                id: row.0,
                topic_name: row.1,
                full_content: row.2,
                compressed_content: row.3,
                created_at: row.4,
                tipcard_type: row.5,
                topic_class: row.6,
                status: row.7,
                next_review_at: row.8,
                repeat_count: serde_json::from_str::<serde_json::Value>(&row.9)
                    .ok()
                    .and_then(|value| value.get("repeats").and_then(|repeats| repeats.as_u64()))
                    .unwrap_or(0) as u32,
            })
            .collect(),
    })
}

async fn app_summary_pb(state: &AppState) -> Result<pb::AppSummary, (StatusCode, String)> {
    let now = Utc::now();
    let topics = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM topics")
        .fetch_one(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let total_cards = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tipcards")
        .fetch_one(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let due_cards = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM review_states WHERE status = 'active' AND next_review_at <= ?",
    )
    .bind(now)
    .fetch_one(&state.db)
    .await
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let active_cards =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM review_states WHERE status = 'active'")
            .fetch_one(&state.db)
            .await
            .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(pb::AppSummary {
        topics,
        total_cards,
        due_cards,
        active_cards,
    })
}

async fn app_topics_pb(state: &AppState) -> Result<pb::AppTopics, (StatusCode, String)> {
    let now = Utc::now();
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            i64,
            i64,
        ),
    >(
        "SELECT top.id,
                top.name,
                tc.name AS class_name,
                tc.tipcard_type,
                top.prompt_template,
                COUNT(t.id) AS total_cards,
                SUM(CASE WHEN r.status = 'active' AND r.next_review_at <= ? THEN 1 ELSE 0 END) AS due_cards,
                SUM(CASE WHEN r.status != 'active' THEN 1 ELSE 0 END) AS completed_cards
         FROM topics top
         LEFT JOIN topic_classes tc ON top.class_id = tc.id
         LEFT JOIN tipcards t ON t.topic_id = top.id
         LEFT JOIN review_states r ON r.card_id = t.id
         GROUP BY top.id, top.name, top.prompt_template, tc.name, tc.tipcard_type
         ORDER BY due_cards DESC, top.name ASC",
    )
    .bind(now)
    .fetch_all(&state.db)
    .await
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(pb::AppTopics {
        topics: rows
            .into_iter()
            .map(|row| pb::AppTopicInfo {
                id: row.0,
                name: row.1,
                class_name: row.2.unwrap_or_else(|| "default".to_string()),
                tipcard_type: row.3.unwrap_or_else(|| "srs_tip".to_string()),
                prompt_template: row.4.unwrap_or_default(),
                total_cards: row.5,
                due_cards: row.6,
                completed_cards: row.7,
            })
            .collect(),
    })
}

async fn update_topic_prompt(
    state: &AppState,
    req: pb::UpdateTopicRequest,
) -> Result<(), (StatusCode, String)> {
    let prompt_template = req
        .prompt_template
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let result = sqlx::query("UPDATE topics SET prompt_template = ? WHERE id = ?")
        .bind(prompt_template)
        .bind(req.id)
        .execute(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Topic not found".to_string()));
    }

    Ok(())
}

fn normalize_tipcard_type(value: &str, class_name: &str) -> String {
    match value.trim() {
        "casual" | "casual_tip" => "casual_tip".to_string(),
        "repeatable" | "repeatable_tip" | "reword" | "re:word" => "repeatable_tip".to_string(),
        "srs" | "srs_tip" => "srs_tip".to_string(),
        "" if matches!(class_name.trim(), "casual" | "casual_tip") => "casual_tip".to_string(),
        "" if matches!(class_name.trim(), "repeatable" | "reword" | "re:word") => {
            "repeatable_tip".to_string()
        }
        _ => "srs_tip".to_string(),
    }
}

async fn get_or_create_topic_class(
    state: &AppState,
    class_name: &str,
    requested_type: &str,
) -> Result<TopicClassInfo, (StatusCode, String)> {
    let name = if class_name.trim().is_empty() {
        "default"
    } else {
        class_name.trim()
    };

    if let Some(row) = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, tipcard_type FROM topic_classes WHERE name = ?",
    )
    .bind(name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        return Ok(TopicClassInfo {
            id: row.0,
            name: name.to_string(),
            tipcard_type: row.1,
        });
    }

    let tipcard_type = normalize_tipcard_type(requested_type, name);
    let id = sqlx::query("INSERT INTO topic_classes (name, tipcard_type) VALUES (?, ?)")
        .bind(name)
        .bind(&tipcard_type)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .last_insert_rowid();

    Ok(TopicClassInfo {
        id,
        name: name.to_string(),
        tipcard_type,
    })
}

async fn get_or_create_topic(
    state: &AppState,
    topic_name: &str,
    class_id: i64,
) -> Result<TopicInfo, (StatusCode, String)> {
    if let Some(row) = sqlx::query_as::<_, (i64, Option<String>)>(
        "SELECT id, prompt_template FROM topics WHERE name = ? AND class_id = ?",
    )
    .bind(topic_name)
    .bind(class_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        return Ok(TopicInfo {
            id: row.0,
            prompt_template: row.1,
        });
    }

    match sqlx::query("INSERT INTO topics (name, class_id) VALUES (?, ?)")
        .bind(topic_name)
        .bind(class_id)
        .execute(&state.db)
        .await
    {
        Ok(result) => Ok(TopicInfo {
            id: result.last_insert_rowid(),
            prompt_template: None,
        }),
        Err(insert_error) => {
            if let Some(row) = sqlx::query_as::<_, (i64, Option<String>)>(
                "SELECT id, prompt_template FROM topics WHERE name = ? AND class_id = ?",
            )
            .bind(topic_name)
            .bind(class_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            {
                Ok(TopicInfo {
                    id: row.0,
                    prompt_template: row.1,
                })
            } else {
                Err((StatusCode::INTERNAL_SERVER_ERROR, insert_error.to_string()))
            }
        }
    }
}

fn tip_response_json(
    id: i64,
    topic: &str,
    full_content: String,
    compressed_content: String,
    class_info: &TopicClassInfo,
) -> TipCardJson {
    TipCardJson {
        id,
        topic: topic.to_string(),
        full_content,
        compressed_content,
        topic_class: class_info.name.clone(),
        tipcard_type: class_info.tipcard_type.clone(),
    }
}

pub async fn build_tips(
    state: &AppState,
    query: TipsJsonRequest,
) -> Result<Vec<TipCardJson>, (StatusCode, String)> {
    let count = query.count.unwrap_or(1).max(1);
    let topics: Vec<&str> = query.topics.split(',').collect();
    let mut responses = Vec::new();
    let topic_class = query.topic_class.unwrap_or_default();
    let tipcard_type = query.tipcard_type.unwrap_or_default();
    let exclude_card_ids: Vec<i64> = query
        .exclude_card_ids
        .unwrap_or_default()
        .into_iter()
        .filter(|id| *id > 0)
        .collect();
    let class_info = get_or_create_topic_class(state, &topic_class, &tipcard_type).await?;

    let settings_str = fs::read_to_string(&state.settings_path).unwrap_or_default();
    let settings: serde_yaml::Value = serde_yaml::from_str(&settings_str).unwrap_or_default();
    let llm_model = settings
        .get("llm_model")
        .and_then(|v| v.as_str())
        .unwrap_or("google/gemini-3.1-flash")
        .to_string();
    let template = settings
        .get("prompt_template")
        .and_then(|v| v.as_str())
        .unwrap_or("Give a smart tip about {topic}.")
        .to_string();
    let llm_api_key = settings
        .get("llm_api_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let llm_base_url = settings
        .get("llm_base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://openrouter.ai/api/v1")
        .to_string();
    let llm_compress_model = settings
        .get("llm_compress_model")
        .and_then(|v| v.as_str())
        .unwrap_or("google/gemini-3.1-flash-lite-preview")
        .to_string();
    let llm_compress_base_url = settings
        .get("llm_compress_base_url")
        .and_then(|v| v.as_str())
        .unwrap_or(&llm_base_url)
        .to_string();
    let llm_reasoning_effort = settings
        .get("llm_reasoning_effort")
        .and_then(|v| v.as_str())
        .unwrap_or("none")
        .to_string();
    let llm_compress_reasoning_effort = settings
        .get("llm_compress_reasoning_effort")
        .and_then(|v| v.as_str())
        .unwrap_or("none")
        .to_string();
    let llm_reasoning = llm::ReasoningConfig::new(llm_reasoning_effort);
    let llm_compress_reasoning = llm::ReasoningConfig::new(llm_compress_reasoning_effort);

    for topic_name in topics.into_iter().take(count as usize) {
        let topic_name = topic_name.trim();
        if topic_name.is_empty() {
            continue;
        }

        let topic = get_or_create_topic(state, topic_name, class_info.id).await?;

        let now = Utc::now();
        let mut due_query = QueryBuilder::<Sqlite>::new(
            "
            SELECT t.id, t.full_content, t.compressed_content
            FROM tipcards t
            JOIN review_states r ON t.id = r.card_id
            WHERE t.topic_id = ",
        );
        due_query.push_bind(topic.id);
        due_query.push(
            "
              AND t.tipcard_type = ",
        );
        due_query.push_bind(&class_info.tipcard_type);
        due_query.push(
            "
              AND r.status = 'active'
              AND r.next_review_at <= ",
        );
        due_query.push_bind(now);
        if !exclude_card_ids.is_empty() {
            due_query.push(" AND t.id NOT IN (");
            let mut separated = due_query.separated(", ");
            for id in &exclude_card_ids {
                separated.push_bind(id);
            }
            separated.push_unseparated(")");
        }
        due_query.push(
            "
            ORDER BY
                CASE
                    WHEN t.tipcard_type = 'repeatable_tip'
                         AND COALESCE(CAST(json_extract(r.state_data, '$.repeats') AS INTEGER), 0) > 0
                    THEN 0
                    ELSE 1
                END ASC,
                r.next_review_at ASC
            LIMIT 1
            ",
        );

        let due_card = due_query
            .build_query_as::<(i64, String, String)>()
            .fetch_optional(&state.db)
            .await
            .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some(card) = due_card {
            responses.push(tip_response_json(
                card.0,
                topic_name,
                card.1,
                card.2,
                &class_info,
            ));
        } else {
            let template = topic
                .prompt_template
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&template);
            let card_context =
                context::load_card_context(state, topic.id, &class_info.tipcard_type).await?;
            let prompt = context::render_generation_prompt(topic_name, template, &card_context);
            let full_tip = llm::generate_new_card(
                &llm_model,
                &prompt,
                &llm_api_key,
                &llm_base_url,
                &llm_reasoning,
            )
            .await;
            let compressed_tip = llm::compress_card(
                &full_tip,
                &llm_compress_model,
                &llm_api_key,
                &llm_compress_base_url,
                &llm_compress_reasoning,
            )
            .await;
            let card_title = llm::generate_card_title(
                &full_tip,
                &llm_compress_model,
                &llm_api_key,
                &llm_compress_base_url,
                &llm_compress_reasoning,
            )
            .await;

            let card_id = sqlx::query(
                "INSERT INTO tipcards (topic_id, tipcard_type, title, full_content, compressed_content) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(topic.id)
            .bind(&class_info.tipcard_type)
            .bind(&card_title)
            .bind(&full_tip)
            .bind(&compressed_tip)
            .execute(&state.db).await.map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
             .last_insert_rowid();

            let (state_json, algo) = if is_queue_tipcard(&class_info.tipcard_type) {
                let algo = if class_info.tipcard_type == "casual_tip" {
                    "casual"
                } else {
                    "repeatable"
                };
                (
                    serde_json::to_string(&RepeatableState::default()).unwrap(),
                    algo,
                )
            } else {
                let init_state = SrsState::default();
                let algo = match init_state.algorithm {
                    Algorithm::SM2 => "sm2",
                    Algorithm::FSRS => "fsrs",
                };
                (serde_json::to_string(&init_state).unwrap(), algo)
            };

            let now = Utc::now();
            sqlx::query(
                "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES (?, ?, ?, 'active', ?)",
            )
            .bind(card_id)
            .bind(algo)
            .bind(state_json)
            .bind(now)
            .execute(&state.db).await.map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            responses.push(tip_response_json(
                card_id,
                topic_name,
                full_tip,
                compressed_tip,
                &class_info,
            ));
        }
    }

    Ok(responses)
}

pub async fn unified_api(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Response, (StatusCode, String)> {
    let request =
        pb::ApiRequest::decode(body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let op = request
        .op
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Missing API operation".to_string()))?;

    let response = match op {
        pb::api_request::Op::BootstrapApiKey(req) => {
            let settings = read_settings_value(&state);
            let real_token = settings
                .get("admin_token")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if real_token.is_empty() || req.admin_token != real_token {
                return Err((StatusCode::UNAUTHORIZED, "Invalid admin token".to_string()));
            }
            let api_key = create_raw_api_key(&state, Some(req.client_name)).await?;
            pb::ApiResponse {
                result: Some(pb::api_response::Result::ApiKeyCreated(pb::ApiKeyCreated {
                    api_key,
                })),
            }
        }
        other => {
            require_api_key(&state, &request.auth).await?;
            match other {
                pb::api_request::Op::Tips(query) => {
                    let responses = build_tips(
                        &state,
                        TipsJsonRequest {
                            count: Some(query.count as u32),
                            topics: query.topics,
                            topic_class: Some(query.topic_class),
                            tipcard_type: Some(query.tipcard_type),
                            exclude_card_ids: Some(query.exclude_card_ids),
                        },
                    )
                    .await?
                    .into_iter()
                    .map(|card| pb::TipCardResponse {
                        id: card.id,
                        topic: card.topic,
                        full_content: card.full_content,
                        compressed_content: card.compressed_content,
                        topic_class: card.topic_class,
                        tipcard_type: card.tipcard_type,
                    })
                    .collect();
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::Tips(pb::TipsResponse {
                            tips: responses,
                        })),
                    }
                }
                pb::api_request::Op::Review(payload) => {
                    apply_review(
                        &state,
                        payload.card_id,
                        payload.grade as u8,
                        &payload.action,
                    )
                    .await?;
                    empty_response()
                }
                pb::api_request::Op::GetTopics(_) => {
                    let rows = sqlx::query_scalar::<_, String>(
                        "SELECT name FROM topics ORDER BY name ASC",
                    )
                    .fetch_all(&state.db)
                    .await
                    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::Topics(pb::GetTopicsResponse {
                            topics: rows,
                        })),
                    }
                }
                pb::api_request::Op::GetTopicClasses(_) => {
                    let rows = sqlx::query_as::<_, (i64, String, String)>(
                        "SELECT id, name, tipcard_type FROM topic_classes ORDER BY name ASC",
                    )
                    .fetch_all(&state.db)
                    .await
                    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::TopicClasses(
                            pb::GetTopicClassesResponse {
                                classes: rows
                                    .into_iter()
                                    .map(|row| pb::TopicClass {
                                        id: row.0,
                                        name: row.1,
                                        tipcard_type: row.2,
                                    })
                                    .collect(),
                            },
                        )),
                    }
                }
                pb::api_request::Op::GetSettings(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::Settings(current_settings(&state))),
                },
                pb::api_request::Op::UpdateSettings(req) => {
                    update_settings_file(&state, req)?;
                    empty_response()
                }
                pb::api_request::Op::CreateApiKey(req) => {
                    let api_key = create_raw_api_key(&state, Some(req.client_name)).await?;
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::ApiKeyCreated(pb::ApiKeyCreated {
                            api_key,
                        })),
                    }
                }
                pb::api_request::Op::ListApiKeys(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::ApiKeys(
                        list_api_keys_pb(&state).await?,
                    )),
                },
                pb::api_request::Op::DeleteApiKey(req) => {
                    delete_api_key_by_id(&state, req.id).await?;
                    empty_response()
                }
                pb::api_request::Op::ListAdminTopics(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::AdminTopics(
                        list_admin_topics_pb(&state).await?,
                    )),
                },
                pb::api_request::Op::ListTipcards(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::Tipcards(
                        list_tipcards_pb(&state).await?,
                    )),
                },
                pb::api_request::Op::DeleteTipcard(req) => {
                    delete_tipcard_by_id(&state, req.id).await?;
                    empty_response()
                }
                pb::api_request::Op::GetSummary(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::Summary(
                        app_summary_pb(&state).await?,
                    )),
                },
                pb::api_request::Op::ListAppTopics(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::AppTopics(
                        app_topics_pb(&state).await?,
                    )),
                },
                pb::api_request::Op::UpdateTopic(req) => {
                    update_topic_prompt(&state, req).await?;
                    empty_response()
                }
                pb::api_request::Op::BootstrapApiKey(_) => unreachable!(),
            }
        }
    };

    Ok(protobuf_response(&response))
}

pub async fn apply_review(
    state: &AppState,
    card_id: i64,
    grade: u8,
    action: &str,
) -> Result<(), (StatusCode, String)> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT r.state_data, t.tipcard_type
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE r.card_id = ?",
    )
    .bind(card_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = row {
        if is_queue_tipcard(&row.1) {
            let action = action.trim();
            let (new_state_json, status, next_review) = match action {
                "acknowledge" | "acknowledged" => (
                    row.0,
                    "acknowledged".to_string(),
                    Utc::now() + Duration::days(36500),
                ),
                "memorize" => (
                    row.0,
                    "memorized".to_string(),
                    Utc::now() + Duration::days(36500),
                ),
                "dismiss" => (
                    row.0,
                    "dismissed".to_string(),
                    Utc::now() + Duration::days(36500),
                ),
                _ => {
                    let mut repeat_state: RepeatableState =
                        serde_json::from_str(&row.0).map_err(|_| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Invalid repeatable state data".into(),
                            )
                        })?;
                    repeat_state.repeats += 1;
                    let delay_minutes = 10_i64
                        .saturating_mul(
                            2_i64.saturating_pow(repeat_state.repeats.saturating_sub(1)),
                        )
                        .min(24 * 60);
                    (
                        serde_json::to_string(&repeat_state).unwrap(),
                        "active".to_string(),
                        Utc::now() + Duration::minutes(delay_minutes),
                    )
                }
            };

            sqlx::query(
                "UPDATE review_states SET state_data = ?, status = ?, next_review_at = ? WHERE card_id = ?",
            )
            .bind(new_state_json)
            .bind(status)
            .bind(next_review)
            .bind(card_id)
            .execute(&state.db).await.map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        } else {
            let mut srs_state: SrsState = serde_json::from_str(&row.0).map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Invalid state data".into(),
                )
            })?;
            let next_review = srs::calculate_next_review(&mut srs_state, grade);
            let new_state_json = serde_json::to_string(&srs_state).unwrap();

            sqlx::query(
                "UPDATE review_states SET state_data = ?, next_review_at = ? WHERE card_id = ?",
            )
            .bind(new_state_json)
            .bind(next_review)
            .bind(card_id)
            .execute(&state.db)
            .await
            .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        Ok(())
    } else {
        Err((
            StatusCode::NOT_FOUND,
            "Card not found in user reviews".to_string(),
        ))
    }
}

fn is_queue_tipcard(tipcard_type: &str) -> bool {
    matches!(tipcard_type, "casual_tip" | "repeatable_tip")
}
