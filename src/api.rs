use axum::{
    body::Bytes,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;

use crate::{
    llm,
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

#[derive(Deserialize)]
pub struct TipsJsonRequest {
    pub count: Option<u32>,
    pub topics: String,
    pub topic_class: Option<String>,
    pub tipcard_type: Option<String>,
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
) -> Result<i64, (StatusCode, String)> {
    if let Some(id) =
        sqlx::query_scalar::<_, i64>("SELECT id FROM topics WHERE name = ? AND class_id = ?")
            .bind(topic_name)
            .bind(class_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        return Ok(id);
    }

    match sqlx::query("INSERT INTO topics (name, class_id) VALUES (?, ?)")
        .bind(topic_name)
        .bind(class_id)
        .execute(&state.db)
        .await
    {
        Ok(result) => Ok(result.last_insert_rowid()),
        Err(insert_error) => {
            if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM topics WHERE name = ?")
                .bind(topic_name)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            {
                Ok(id)
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

    for topic_name in topics.into_iter().take(count as usize) {
        let topic_name = topic_name.trim();
        if topic_name.is_empty() {
            continue;
        }

        let topic_id = get_or_create_topic(state, topic_name, class_info.id).await?;

        let now = Utc::now();
        let due_card = sqlx::query_as::<_, (i64, String, String)>(
            "
            SELECT t.id, t.full_content, t.compressed_content
            FROM tipcards t
            JOIN review_states r ON t.id = r.card_id
            WHERE t.topic_id = ?
              AND t.tipcard_type = ?
              AND r.status = 'active'
              AND r.next_review_at <= ?
            ORDER BY r.next_review_at ASC LIMIT 1
            ",
        )
        .bind(topic_id)
        .bind(&class_info.tipcard_type)
        .bind(now)
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
            let full_tip = llm::generate_new_card(
                topic_name,
                &llm_model,
                &template,
                &llm_api_key,
                &llm_base_url,
            )
            .await;
            let compressed_tip = llm::compress_card(&full_tip, &llm_api_key, &llm_base_url).await;

            let card_id = sqlx::query(
                "INSERT INTO tipcards (topic_id, tipcard_type, full_content, compressed_content) VALUES (?, ?, ?, ?)",
            )
            .bind(topic_id)
            .bind(&class_info.tipcard_type)
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

pub async fn get_tips(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Response, (StatusCode, String)> {
    let query =
        pb::TipsQuery::decode(body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let responses = build_tips(
        &state,
        TipsJsonRequest {
            count: Some(query.count as u32),
            topics: query.topics,
            topic_class: Some(query.topic_class),
            tipcard_type: Some(query.tipcard_type),
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
    let tips_response = pb::TipsResponse { tips: responses };
    Ok(protobuf_response(&tips_response))
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

pub async fn review_card(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    let payload =
        pb::ReviewPayload::decode(body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    apply_review(
        &state,
        payload.card_id,
        payload.grade as u8,
        &payload.action,
    )
    .await?;
    Ok(StatusCode::OK)
}

fn is_queue_tipcard(tipcard_type: &str) -> bool {
    matches!(tipcard_type, "casual_tip" | "repeatable_tip")
}

pub async fn get_topics(
    State(state): State<Arc<AppState>>,
) -> Result<Response, (StatusCode, String)> {
    let rows = sqlx::query_scalar::<_, String>("SELECT name FROM topics ORDER BY name ASC")
        .fetch_all(&state.db)
        .await
        .map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let topics = rows;
    let response = pb::GetTopicsResponse { topics };
    Ok(protobuf_response(&response))
}

pub async fn get_topic_classes(
    State(state): State<Arc<AppState>>,
) -> Result<Response, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, name, tipcard_type FROM topic_classes ORDER BY name ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let classes = rows
        .into_iter()
        .map(|r| pb::TopicClass {
            id: r.0,
            name: r.1,
            tipcard_type: r.2,
        })
        .collect();
    let response = pb::GetTopicClassesResponse { classes };
    Ok(protobuf_response(&response))
}
