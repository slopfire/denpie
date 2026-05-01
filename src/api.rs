use axum::{
    body::Bytes,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, LocalResult, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
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

fn setting_string(settings: &serde_yaml::Value, key: &str, default: &str) -> String {
    settings
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

fn setting_u64(settings: &serde_yaml::Value, key: &str, default: u64) -> u64 {
    settings
        .get(key)
        .and_then(|v| v.as_u64())
        .unwrap_or(default)
}

fn parse_daily_update_time(value: &str) -> NaiveTime {
    NaiveTime::parse_from_str(value.trim(), "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(value.trim(), "%H:%M:%S"))
        .unwrap_or(NaiveTime::MIN)
}

fn resolve_local_time(tz: Tz, local: chrono::NaiveDateTime) -> DateTime<Utc> {
    for offset_minutes in 0..180 {
        let candidate = local + Duration::minutes(offset_minutes);
        match tz.from_local_datetime(&candidate) {
            LocalResult::Single(dt) => return dt.with_timezone(&Utc),
            LocalResult::Ambiguous(earliest, _) => return earliest.with_timezone(&Utc),
            LocalResult::None => {}
        }
    }
    Utc::now()
}

fn parse_utc_offset_seconds(value: &str) -> Option<i64> {
    let value = value.trim().to_ascii_uppercase();
    let rest = value
        .strip_prefix("UTC")
        .or_else(|| value.strip_prefix("GMT"))?;
    if rest.is_empty() {
        return Some(0);
    }
    let (sign, rest) = match rest.as_bytes().first()? {
        b'+' => (1_i64, &rest[1..]),
        b'-' => (-1_i64, &rest[1..]),
        _ => return None,
    };
    let (hours, minutes) = if let Some((hours, minutes)) = rest.split_once(':') {
        (hours.parse::<i64>().ok()?, minutes.parse::<i64>().ok()?)
    } else {
        (rest.parse::<i64>().ok()?, 0)
    };
    if !(0..=14).contains(&hours) || !(0..60).contains(&minutes) {
        return None;
    }
    Some(sign * (hours * 3600 + minutes * 60))
}

fn daily_window_start(time_zone: &str, update_time: &str) -> DateTime<Utc> {
    if let Some(offset_seconds) = parse_utc_offset_seconds(time_zone) {
        let update_time = parse_daily_update_time(update_time);
        let local_now = (Utc::now() + Duration::seconds(offset_seconds)).naive_utc();
        let mut start_date = local_now.date();
        if local_now.time() < update_time {
            start_date = start_date
                .checked_sub_signed(Duration::days(1))
                .unwrap_or(start_date);
        }
        let start_utc = start_date.and_time(update_time) - Duration::seconds(offset_seconds);
        return DateTime::<Utc>::from_naive_utc_and_offset(start_utc, Utc);
    }

    let tz = time_zone.parse::<Tz>().unwrap_or(chrono_tz::UTC);
    let update_time = parse_daily_update_time(update_time);
    let local_now = Utc::now().with_timezone(&tz);
    let mut start_date = local_now.date_naive();
    if local_now.time() < update_time {
        start_date = start_date
            .checked_sub_signed(Duration::days(1))
            .unwrap_or(start_date);
    }
    resolve_local_time(tz, start_date.and_time(update_time))
}

fn topic_daily_window_start(topic: &TopicInfo, settings: &serde_yaml::Value) -> DateTime<Utc> {
    let default_tz = settings
        .get("daily_time_zone")
        .and_then(|v| v.as_str())
        .unwrap_or("UTC");
    let default_time = settings
        .get("daily_update_time")
        .and_then(|v| v.as_str())
        .unwrap_or("00:00");
    daily_window_start(
        topic
            .daily_time_zone
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(default_tz),
        topic
            .daily_update_time
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(default_time),
    )
}

fn topic_daily_card_count(topic: &TopicInfo) -> usize {
    topic
        .daily_card_count
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or(1)
        .min(20)
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
            .unwrap_or("master")
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
        daily_time_zone: setting_string(&settings, "daily_time_zone", "UTC"),
        daily_update_time: setting_string(&settings, "daily_update_time", "00:00"),
        max_active_cards: setting_u64(&settings, "max_active_cards", 0),
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
        put_string_setting(map, "daily_time_zone", req.daily_time_zone);
        put_string_setting(map, "daily_update_time", req.daily_update_time);
        put_u64_setting(map, "max_active_cards", req.max_active_cards);
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
    #[serde(default)]
    srs_state: SrsState,
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
    daily_card_count: Option<i64>,
    daily_time_zone: Option<String>,
    daily_update_time: Option<String>,
}

#[derive(Deserialize)]
pub struct TipsJsonRequest {
    pub count: Option<u32>,
    pub topics: String,
    pub topic_class: Option<String>,
    pub tipcard_type: Option<String>,
    pub exclude_card_ids: Option<Vec<i64>>,
    pub manual_content: Option<String>,
    pub manual_compressed_content: Option<String>,
    pub manual_image_data: Option<Vec<String>>,
}

#[derive(Clone, Serialize)]
pub struct TipCardJson {
    pub id: i64,
    pub topic: String,
    pub full_content: String,
    pub compressed_content: String,
    pub image_data: Vec<String>,
    pub topic_class: String,
    pub tipcard_type: String,
    pub pinned: bool,
}

#[derive(Deserialize)]
pub struct ReviewJsonRequest {
    pub card_id: i64,
    pub grade: Option<u8>,
    pub action: Option<String>,
}

#[derive(Deserialize)]
pub struct ForceDailyRefreshRequest {
    pub topics: String,
    pub topic_class: Option<String>,
    pub tipcard_type: Option<String>,
}

#[derive(Serialize)]
pub struct ForceDailyRefreshResponse {
    pub refreshed_cards: u64,
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
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            String,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT id, name, prompt_template, daily_card_count, daily_time_zone, daily_update_time
         FROM topics
         ORDER BY name ASC",
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
                daily_card_count: row.3.unwrap_or(1).max(1) as u32,
                daily_time_zone: row.4.unwrap_or_default(),
                daily_update_time: row.5.unwrap_or_default(),
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

pub async fn set_tipcard_pinned(
    state: &AppState,
    id: i64,
    pinned: bool,
) -> Result<(), (StatusCode, String)> {
    let result = sqlx::query("UPDATE tipcards SET pinned = ? WHERE id = ?")
        .bind(if pinned { 1 } else { 0 })
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Tipcard not found".to_string()));
    }
    Ok(())
}

pub async fn delete_topic_by_id(state: &AppState, id: i64) -> Result<(), (StatusCode, String)> {
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    sqlx::query(
        "DELETE FROM review_states
         WHERE card_id IN (SELECT id FROM tipcards WHERE topic_id = ?)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    sqlx::query("DELETE FROM tipcards WHERE topic_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let result = sqlx::query("DELETE FROM topics WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Topic not found".to_string()));
    }

    tx.commit()
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

async fn record_llm_token_usage(
    state: &AppState,
    model: &str,
    purpose: &str,
    usage: &llm::TokenUsage,
) -> Result<(), (StatusCode, String)> {
    if usage.total_tokens <= 0 && usage.prompt_tokens <= 0 && usage.completion_tokens <= 0 {
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO llm_token_usage (model, purpose, prompt_tokens, completion_tokens, total_tokens)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(model)
    .bind(purpose)
    .bind(usage.prompt_tokens)
    .bind(usage.completion_tokens)
    .bind(usage.total_tokens)
    .execute(&state.db)
    .await
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(())
}

pub async fn set_tipcard_images(
    state: &AppState,
    id: i64,
    image_data: Vec<String>,
) -> Result<(), (StatusCode, String)> {
    let image_data = validate_image_data(image_data)?;
    let image_data_json = image_data_json(&image_data)?;
    let result = sqlx::query("UPDATE tipcards SET image_data = ? WHERE id = ?")
        .bind(image_data_json)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Card not found".to_string()));
    }

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
            i64,
        ),
    >(
        "SELECT t.id,
                top.name AS topic_name,
                t.full_content,
                t.compressed_content,
                COALESCE(CAST(t.created_at AS TEXT), '') AS created_at,
                t.tipcard_type,
                COALESCE(tc.name, 'default') AS topic_class,
                COALESCE(r.status, CASE WHEN t.tipcard_type = 'custom_tip' THEN 'custom' ELSE 'active' END) AS status,
                COALESCE(CAST(r.next_review_at AS TEXT), '') AS next_review_at,
                COALESCE(r.state_data, '') AS state_data,
                COALESCE(t.pinned, 0) AS pinned
         FROM tipcards t
         JOIN topics top ON t.topic_id = top.id
         LEFT JOIN topic_classes tc ON top.class_id = tc.id
         LEFT JOIN review_states r ON r.card_id = t.id
         ORDER BY pinned DESC, t.created_at DESC",
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
                pinned: row.10 != 0,
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
        "SELECT COUNT(*)
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE r.status = 'active' AND (r.next_review_at <= ? OR t.pinned = 1)",
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
            Option<i64>,
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
                top.daily_card_count,
                top.daily_time_zone,
                top.daily_update_time,
                COUNT(t.id) AS total_cards,
                SUM(CASE WHEN r.status = 'active' AND (r.next_review_at <= ? OR t.pinned = 1) THEN 1 ELSE 0 END) AS due_cards,
                SUM(CASE WHEN r.status != 'active' THEN 1 ELSE 0 END) AS completed_cards
         FROM topics top
         LEFT JOIN topic_classes tc ON top.class_id = tc.id
         LEFT JOIN tipcards t ON t.topic_id = top.id
         LEFT JOIN review_states r ON r.card_id = t.id
         GROUP BY top.id, top.name, top.prompt_template, top.daily_card_count, top.daily_time_zone, top.daily_update_time, tc.name, tc.tipcard_type
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
                daily_card_count: row.5.unwrap_or(1).max(1) as u32,
                daily_time_zone: row.6.unwrap_or_default(),
                daily_update_time: row.7.unwrap_or_default(),
                total_cards: row.8,
                due_cards: row.9,
                completed_cards: row.10,
            })
            .collect(),
    })
}

async fn update_topic_prompt(
    state: &AppState,
    req: pb::UpdateTopicRequest,
) -> Result<(), (StatusCode, String)> {
    let current =
        sqlx::query_as::<_, (Option<String>, Option<i64>, Option<String>, Option<String>)>(
            "SELECT prompt_template, daily_card_count, daily_time_zone, daily_update_time
         FROM topics
         WHERE id = ?",
        )
        .bind(req.id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Topic not found".to_string()))?;

    let prompt_template = req
        .prompt_template
        .map(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        })
        .unwrap_or(current.0);
    let daily_card_count = req
        .daily_card_count
        .map(|value| {
            if value == 0 {
                None
            } else {
                Some(i64::from(value))
            }
        })
        .unwrap_or(current.1);
    let daily_time_zone = req
        .daily_time_zone
        .map(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        })
        .unwrap_or(current.2);
    let daily_update_time = req
        .daily_update_time
        .map(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        })
        .unwrap_or(current.3);

    sqlx::query(
        "UPDATE topics
         SET prompt_template = ?, daily_card_count = ?, daily_time_zone = ?, daily_update_time = ?
         WHERE id = ?",
    )
    .bind(prompt_template)
    .bind(daily_card_count)
    .bind(daily_time_zone)
    .bind(daily_update_time)
    .bind(req.id)
    .execute(&state.db)
    .await
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(())
}

fn normalize_tipcard_type(value: &str, class_name: &str) -> String {
    match value.trim() {
        "casual" | "casual_tip" => "casual_tip".to_string(),
        "repeatable" | "repeatable_tip" | "reword" | "re:word" => "repeatable_tip".to_string(),
        "manual" | "manual_tip" => "manual_tip".to_string(),
        "custom" | "custom_tip" => "custom_tip".to_string(),
        "srs" | "srs_tip" => "srs_tip".to_string(),
        "" if matches!(class_name.trim(), "casual" | "casual_tip") => "casual_tip".to_string(),
        "" if matches!(class_name.trim(), "repeatable" | "reword" | "re:word") => {
            "repeatable_tip".to_string()
        }
        "" if matches!(class_name.trim(), "manual" | "manual_tip") => "manual_tip".to_string(),
        "" if matches!(class_name.trim(), "custom" | "custom_tip") => "custom_tip".to_string(),
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
    if let Some(row) = sqlx::query_as::<
        _,
        (
            i64,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT id, prompt_template, daily_card_count, daily_time_zone, daily_update_time
         FROM topics
         WHERE name = ? AND class_id = ?",
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
            daily_card_count: row.2,
            daily_time_zone: row.3,
            daily_update_time: row.4,
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
            daily_card_count: None,
            daily_time_zone: None,
            daily_update_time: None,
        }),
        Err(insert_error) => {
            if let Some(row) = sqlx::query_as::<
                _,
                (
                    i64,
                    Option<String>,
                    Option<i64>,
                    Option<String>,
                    Option<String>,
                ),
            >(
                "SELECT id, prompt_template, daily_card_count, daily_time_zone, daily_update_time
                 FROM topics
                 WHERE name = ? AND class_id = ?",
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
                    daily_card_count: row.2,
                    daily_time_zone: row.3,
                    daily_update_time: row.4,
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
    image_data: Vec<String>,
    class_info: &TopicClassInfo,
    pinned: bool,
) -> TipCardJson {
    TipCardJson {
        id,
        topic: topic.to_string(),
        full_content,
        compressed_content,
        image_data,
        topic_class: class_info.name.clone(),
        tipcard_type: class_info.tipcard_type.clone(),
        pinned,
    }
}

fn parse_image_data(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn image_data_json(images: &[String]) -> Result<String, (StatusCode, String)> {
    serde_json::to_string(images).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub fn validate_image_data(images: Vec<String>) -> Result<Vec<String>, (StatusCode, String)> {
    const MAX_IMAGES: usize = 4;
    const MAX_TOTAL_CHARS: usize = 12 * 1024 * 1024;
    let mut normalized = Vec::new();
    let mut total_chars = 0usize;

    for image in images {
        let image = image.trim().to_string();
        if image.is_empty() {
            continue;
        }
        if normalized.len() >= MAX_IMAGES {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("A tipcard can have at most {MAX_IMAGES} images"),
            ));
        }
        let allowed = image.starts_with("data:image/png;base64,")
            || image.starts_with("data:image/jpeg;base64,")
            || image.starts_with("data:image/jpg;base64,")
            || image.starts_with("data:image/webp;base64,")
            || image.starts_with("data:image/gif;base64,");
        if !allowed {
            return Err((
                StatusCode::BAD_REQUEST,
                "Only PNG, JPEG, WebP, or GIF data URLs are supported".to_string(),
            ));
        }
        total_chars = total_chars.saturating_add(image.len());
        if total_chars > MAX_TOTAL_CHARS {
            return Err((
                StatusCode::BAD_REQUEST,
                "Attached images are too large".to_string(),
            ));
        }
        normalized.push(image);
    }

    Ok(normalized)
}

async fn find_daily_topic_cards(
    state: &AppState,
    topic_id: i64,
    tipcard_type: &str,
    daily_window_start: DateTime<Utc>,
    exclude_card_ids: &[i64],
    limit: usize,
) -> Result<Vec<(i64, String, String, i64, String)>, (StatusCode, String)> {
    let mut daily_query = QueryBuilder::<Sqlite>::new(
        "
        SELECT t.id, t.full_content, t.compressed_content, COALESCE(t.pinned, 0), COALESCE(t.image_data, '[]')
        FROM tipcards t
        JOIN review_states r ON t.id = r.card_id
        WHERE t.topic_id = ",
    );
    daily_query.push_bind(topic_id);
    daily_query.push(" AND t.tipcard_type = ");
    daily_query.push_bind(tipcard_type);
    daily_query.push(" AND r.status = 'active'");
    daily_query.push(" AND t.created_at >= ");
    daily_query.push_bind(
        daily_window_start
            .naive_utc()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    );
    if !exclude_card_ids.is_empty() {
        daily_query.push(" AND t.id NOT IN (");
        let mut separated = daily_query.separated(", ");
        for id in exclude_card_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
    }
    daily_query.push(" ORDER BY t.pinned DESC, t.created_at ASC LIMIT ");
    daily_query.push_bind(limit as i64);

    daily_query
        .build_query_as::<(i64, String, String, i64, String)>()
        .fetch_all(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn find_due_topic_cards(
    state: &AppState,
    topic_id: i64,
    tipcard_type: &str,
    exclude_card_ids: &[i64],
    limit: usize,
) -> Result<Vec<(i64, String, String, i64, String)>, (StatusCode, String)> {
    let now = Utc::now();
    let mut due_query = QueryBuilder::<Sqlite>::new(
        "
        SELECT t.id, t.full_content, t.compressed_content, COALESCE(t.pinned, 0), COALESCE(t.image_data, '[]')
        FROM tipcards t
        JOIN review_states r ON t.id = r.card_id
        WHERE t.topic_id = ",
    );
    due_query.push_bind(topic_id);
    due_query.push(" AND t.tipcard_type = ");
    due_query.push_bind(tipcard_type);
    due_query.push(" AND r.status = 'active' AND (r.next_review_at <= ");
    due_query.push_bind(now);
    due_query.push(" OR t.pinned = 1)");
    if !exclude_card_ids.is_empty() {
        due_query.push(" AND t.id NOT IN (");
        let mut separated = due_query.separated(", ");
        for id in exclude_card_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
    }
    due_query.push(
        "
        ORDER BY
            t.pinned DESC,
            CASE
                WHEN t.tipcard_type = 'repeatable_tip'
                     AND COALESCE(CAST(json_extract(r.state_data, '$.repeats') AS INTEGER), 0) > 0
                THEN 0
                ELSE 1
            END ASC,
            r.next_review_at ASC
        LIMIT ",
    );
    due_query.push_bind(limit as i64);

    due_query
        .build_query_as::<(i64, String, String, i64, String)>()
        .fetch_all(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn active_card_count(state: &AppState) -> Result<i64, (StatusCode, String)> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM review_states WHERE status = 'active'")
        .fetch_one(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn active_card_room(
    state: &AppState,
    max_active_cards: u64,
) -> Result<Option<usize>, (StatusCode, String)> {
    if max_active_cards == 0 {
        return Ok(None);
    }
    let active = active_card_count(state).await?.max(0) as u64;
    Ok(Some(max_active_cards.saturating_sub(active) as usize))
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
    let manual_content = query.manual_content.unwrap_or_default().trim().to_string();
    let manual_compressed_content = query
        .manual_compressed_content
        .unwrap_or_default()
        .trim()
        .to_string();
    let manual_image_data = validate_image_data(query.manual_image_data.unwrap_or_default())?;
    let exclude_card_ids: Vec<i64> = query
        .exclude_card_ids
        .unwrap_or_default()
        .into_iter()
        .filter(|id| *id > 0)
        .collect();
    let class_info = get_or_create_topic_class(state, &topic_class, &tipcard_type).await?;
    if class_info.tipcard_type == "custom_tip" {
        return Err((
            StatusCode::BAD_REQUEST,
            "custom_tip cards must be submitted with submit_custom_tipcard".to_string(),
        ));
    }

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
    let max_active_cards = setting_u64(&settings, "max_active_cards", 0);
    let mut active_room = active_card_room(state, max_active_cards).await?;
    let llm_reasoning = llm::ReasoningConfig::new(llm_reasoning_effort);
    let llm_compress_reasoning = llm::ReasoningConfig::new(llm_compress_reasoning_effort);

    for topic_name in topics.into_iter().take(count as usize) {
        let topic_name = topic_name.trim();
        if topic_name.is_empty() {
            continue;
        }

        let topic = get_or_create_topic(state, topic_name, class_info.id).await?;
        if class_info.tipcard_type == "manual_tip" {
            if manual_content.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "manual_content is required for manual_tip".to_string(),
                ));
            }
            if matches!(active_room, Some(0)) {
                return Err((StatusCode::CONFLICT, "Max active cards reached".to_string()));
            }
            let compact = if manual_compressed_content.is_empty() {
                manual_content.clone()
            } else {
                manual_compressed_content.clone()
            };
            create_manual_tipcard(
                state,
                topic_name,
                &topic,
                &class_info,
                manual_content.clone(),
                compact,
                manual_image_data.clone(),
                &mut responses,
            )
            .await?;
            if let Some(room) = active_room.as_mut() {
                *room = room.saturating_sub(1);
            }
            continue;
        }

        let daily_card_count = if is_queue_tipcard(&class_info.tipcard_type) {
            1
        } else {
            topic_daily_card_count(&topic)
        };

        let due_cards = find_due_topic_cards(
            state,
            topic.id,
            &class_info.tipcard_type,
            &exclude_card_ids,
            daily_card_count,
        )
        .await?;
        for card in &due_cards {
            responses.push(tip_response_json(
                card.0,
                topic_name,
                card.1.clone(),
                card.2.clone(),
                parse_image_data(&card.4),
                &class_info,
                card.3 != 0,
            ));
        }
        if !due_cards.is_empty() {
            continue;
        } else if !is_queue_tipcard(&class_info.tipcard_type) {
            let daily_window_start = topic_daily_window_start(&topic, &settings);
            let daily_cards = find_daily_topic_cards(
                state,
                topic.id,
                &class_info.tipcard_type,
                daily_window_start,
                &exclude_card_ids,
                daily_card_count,
            )
            .await?;
            for card in &daily_cards {
                responses.push(tip_response_json(
                    card.0,
                    topic_name,
                    card.1.clone(),
                    card.2.clone(),
                    parse_image_data(&card.4),
                    &class_info,
                    card.3 != 0,
                ));
            }
            let remaining_daily_cards = daily_card_count.saturating_sub(daily_cards.len());
            let cards_to_generate = active_room.map_or(remaining_daily_cards, |room| {
                remaining_daily_cards.min(room)
            });
            for _ in 0..cards_to_generate {
                generate_tipcard(
                    state,
                    topic_name,
                    &topic,
                    &class_info,
                    &template,
                    &llm_model,
                    &llm_api_key,
                    &llm_base_url,
                    &llm_reasoning,
                    &llm_compress_model,
                    &llm_compress_base_url,
                    &llm_compress_reasoning,
                    &mut responses,
                )
                .await?;
                if let Some(room) = active_room.as_mut() {
                    *room = room.saturating_sub(1);
                }
            }
        } else if active_room.map_or(true, |room| room > 0) {
            generate_tipcard(
                state,
                topic_name,
                &topic,
                &class_info,
                &template,
                &llm_model,
                &llm_api_key,
                &llm_base_url,
                &llm_reasoning,
                &llm_compress_model,
                &llm_compress_base_url,
                &llm_compress_reasoning,
                &mut responses,
            )
            .await?;
            if let Some(room) = active_room.as_mut() {
                *room = room.saturating_sub(1);
            }
        }
    }

    Ok(responses)
}

pub async fn force_daily_refresh(
    state: &AppState,
    req: ForceDailyRefreshRequest,
) -> Result<ForceDailyRefreshResponse, (StatusCode, String)> {
    let topic_class = req.topic_class.unwrap_or_default();
    let tipcard_type = req.tipcard_type.unwrap_or_default();
    let class_info = get_or_create_topic_class(state, &topic_class, &tipcard_type).await?;
    if matches!(
        class_info.tipcard_type.as_str(),
        "manual_tip" | "custom_tip"
    ) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Only generated daily cards can be force-refreshed".to_string(),
        ));
    }

    let settings_str = fs::read_to_string(&state.settings_path).unwrap_or_default();
    let settings: serde_yaml::Value = serde_yaml::from_str(&settings_str).unwrap_or_default();
    let dismissed_until = Utc::now() + Duration::days(36500);
    let mut refreshed_cards = 0u64;

    for topic_name in req.topics.split(',') {
        let topic_name = topic_name.trim();
        if topic_name.is_empty() {
            continue;
        }

        let topic = get_or_create_topic(state, topic_name, class_info.id).await?;
        let result = if is_queue_tipcard(&class_info.tipcard_type) {
            sqlx::query(
                "UPDATE review_states
                 SET status = 'dismissed', next_review_at = ?
                 WHERE card_id IN (
                     SELECT t.id
                     FROM tipcards t
                     WHERE t.topic_id = ?
                       AND t.tipcard_type = ?
                       AND COALESCE(t.pinned, 0) = 0
                 )
                 AND status = 'active'",
            )
            .bind(dismissed_until)
            .bind(topic.id)
            .bind(&class_info.tipcard_type)
            .execute(&state.db)
            .await
        } else {
            let daily_window_start = topic_daily_window_start(&topic, &settings);
            sqlx::query(
                "UPDATE review_states
                 SET status = 'dismissed', next_review_at = ?
                 WHERE card_id IN (
                     SELECT t.id
                     FROM tipcards t
                     WHERE t.topic_id = ?
                       AND t.tipcard_type = ?
                       AND COALESCE(t.pinned, 0) = 0
                       AND t.created_at >= ?
                 )
                 AND status = 'active'",
            )
            .bind(dismissed_until)
            .bind(topic.id)
            .bind(&class_info.tipcard_type)
            .bind(
                daily_window_start
                    .naive_utc()
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
            )
            .execute(&state.db)
            .await
        }
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        refreshed_cards = refreshed_cards.saturating_add(result.rows_affected());
    }

    Ok(ForceDailyRefreshResponse { refreshed_cards })
}

async fn generate_tipcard(
    state: &AppState,
    topic_name: &str,
    topic: &TopicInfo,
    class_info: &TopicClassInfo,
    template: &str,
    llm_model: &str,
    llm_api_key: &str,
    llm_base_url: &str,
    llm_reasoning: &llm::ReasoningConfig,
    llm_compress_model: &str,
    llm_compress_base_url: &str,
    llm_compress_reasoning: &llm::ReasoningConfig,
    responses: &mut Vec<TipCardJson>,
) -> Result<(), (StatusCode, String)> {
    let template = topic
        .prompt_template
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(template);
    let card_context =
        context::load_card_context(state, topic.id, &class_info.tipcard_type).await?;
    let prompt = context::render_generation_prompt(topic_name, template, &card_context);
    let full_res =
        llm::generate_new_card(llm_model, &prompt, llm_api_key, llm_base_url, llm_reasoning).await;
    record_llm_token_usage(state, llm_model, "generate_card", &full_res.usage).await?;
    let full_tip = full_res.content;

    let compressed_res = llm::compress_card(
        &full_tip,
        llm_compress_model,
        llm_api_key,
        llm_compress_base_url,
        llm_compress_reasoning,
    )
    .await;
    record_llm_token_usage(
        state,
        llm_compress_model,
        "compress_card",
        &compressed_res.usage,
    )
    .await?;
    let compressed_tip = compressed_res.content;

    let title_res = llm::generate_card_title(
        &full_tip,
        llm_compress_model,
        llm_api_key,
        llm_compress_base_url,
        llm_compress_reasoning,
    )
    .await;
    record_llm_token_usage(
        state,
        llm_compress_model,
        "generate_title",
        &title_res.usage,
    )
    .await?;
    let card_title = title_res.content;

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
        (
            serde_json::to_string(&RepeatableState::default()).unwrap(),
            "sm2",
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
        Vec::new(),
        class_info,
        false,
    ));

    Ok(())
}

async fn create_manual_tipcard(
    state: &AppState,
    topic_name: &str,
    topic: &TopicInfo,
    class_info: &TopicClassInfo,
    full_tip: String,
    compressed_tip: String,
    image_data: Vec<String>,
    responses: &mut Vec<TipCardJson>,
) -> Result<(), (StatusCode, String)> {
    let title = full_tip
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Manual card")
        .chars()
        .take(96)
        .collect::<String>();

    let image_data_json = image_data_json(&image_data)?;
    let card_id = sqlx::query(
        "INSERT INTO tipcards (topic_id, tipcard_type, title, full_content, compressed_content, image_data) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(topic.id)
    .bind(&class_info.tipcard_type)
    .bind(&title)
    .bind(&full_tip)
    .bind(&compressed_tip)
    .bind(&image_data_json)
    .execute(&state.db)
    .await
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .last_insert_rowid();

    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES (?, 'manual', ?, 'active', ?)",
    )
    .bind(card_id)
    .bind(serde_json::to_string(&RepeatableState::default()).unwrap())
    .bind(Utc::now())
    .execute(&state.db)
    .await
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    responses.push(tip_response_json(
        card_id,
        topic_name,
        full_tip,
        compressed_tip,
        image_data,
        class_info,
        false,
    ));

    Ok(())
}

async fn create_custom_tipcard(
    state: &AppState,
    req: pb::CustomTipcardRequest,
) -> Result<TipCardJson, (StatusCode, String)> {
    let topic_name = req.topic.trim();
    if topic_name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "topic is required".to_string()));
    }

    let full_tip = req.full_content.trim().to_string();
    if full_tip.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "full_content is required".to_string(),
        ));
    }

    let compressed_tip = req.compressed_content.trim().to_string();
    let compressed_tip = if compressed_tip.is_empty() {
        full_tip.clone()
    } else {
        compressed_tip
    };
    let title = req.title.trim().to_string();
    let title = if title.is_empty() {
        full_tip
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("Custom card")
            .chars()
            .take(96)
            .collect::<String>()
    } else {
        title.chars().take(96).collect::<String>()
    };

    let class_info = get_or_create_topic_class(state, "custom", "custom_tip").await?;
    let topic = get_or_create_topic(state, topic_name, class_info.id).await?;
    let card_id = sqlx::query(
        "INSERT INTO tipcards (topic_id, tipcard_type, title, full_content, compressed_content) VALUES (?, 'custom_tip', ?, ?, ?)",
    )
    .bind(topic.id)
    .bind(&title)
    .bind(&full_tip)
    .bind(&compressed_tip)
    .execute(&state.db)
    .await
    .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .last_insert_rowid();

    Ok(tip_response_json(
        card_id,
        topic_name,
        full_tip,
        compressed_tip,
        Vec::new(),
        &class_info,
        false,
    ))
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
                            manual_content: Some(query.manual_content),
                            manual_compressed_content: Some(query.manual_compressed_content),
                            manual_image_data: None,
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
                        pinned: card.pinned,
                    })
                    .collect();
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::Tips(pb::TipsResponse {
                            tips: responses,
                        })),
                    }
                }
                pb::api_request::Op::SubmitCustomTipcard(req) => {
                    let card = create_custom_tipcard(&state, req).await?;
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::Tips(pb::TipsResponse {
                            tips: vec![pb::TipCardResponse {
                                id: card.id,
                                topic: card.topic,
                                full_content: card.full_content,
                                compressed_content: card.compressed_content,
                                topic_class: card.topic_class,
                                tipcard_type: card.tipcard_type,
                                pinned: card.pinned,
                            }],
                        })),
                    }
                }
                pb::api_request::Op::ForceDailyRefresh(req) => {
                    let result = force_daily_refresh(
                        &state,
                        ForceDailyRefreshRequest {
                            topics: req.topics,
                            topic_class: Some(req.topic_class),
                            tipcard_type: Some(req.tipcard_type),
                        },
                    )
                    .await?;
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::ForceDailyRefresh(
                            pb::ForceDailyRefreshResponse {
                                refreshed_cards: result.refreshed_cards,
                            },
                        )),
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
                pb::api_request::Op::PinTipcard(req) => {
                    set_tipcard_pinned(&state, req.id, req.pinned).await?;
                    empty_response()
                }
                pb::api_request::Op::DeleteTopic(req) => {
                    delete_topic_by_id(&state, req.id).await?;
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
                "acknowledge" | "acknowledged" => {
                    let mut repeat_state: RepeatableState =
                        serde_json::from_str(&row.0).map_err(|_| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Invalid repeatable state data".into(),
                            )
                        })?;
                    let next_review =
                        srs::calculate_next_review(&mut repeat_state.srs_state, grade.max(3));
                    (
                        serde_json::to_string(&repeat_state).unwrap(),
                        "active".to_string(),
                        next_review,
                    )
                }
                "memorize" => {
                    let mut repeat_state: RepeatableState =
                        serde_json::from_str(&row.0).map_err(|_| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Invalid repeatable state data".into(),
                            )
                        })?;
                    repeat_state.repeats += 1;
                    let next_review = srs::calculate_next_review(&mut repeat_state.srs_state, 5);
                    (
                        serde_json::to_string(&repeat_state).unwrap(),
                        "active".to_string(),
                        next_review,
                    )
                }
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
                    let next_review = srs::calculate_next_review(
                        &mut repeat_state.srs_state,
                        if grade == 0 { 1 } else { grade.min(2) },
                    );
                    (
                        serde_json::to_string(&repeat_state).unwrap(),
                        "active".to_string(),
                        next_review,
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
    matches!(tipcard_type, "casual_tip" | "repeatable_tip" | "manual_tip")
}
