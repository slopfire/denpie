use axum::{
    body::Bytes,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use prost::Message;
use std::sync::Arc;
use std::fs;
use chrono::Utc;

use crate::{AppState, llm, srs::{self, SrsState, Algorithm}};

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

pub async fn get_tips(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Response, (StatusCode, String)> {
    let query = pb::TipsQuery::decode(body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    
    let topics: Vec<&str> = query.topics.split(',').collect();
    let mut responses = Vec::new();

    let settings_str = fs::read_to_string("settings.yaml").unwrap_or_default();
    let settings: serde_yaml::Value = serde_yaml::from_str(&settings_str).unwrap_or_default();
    let llm_model = settings.get("llm_model").and_then(|v| v.as_str()).unwrap_or("google/gemini-3.1-flash").to_string();
    let template = settings.get("prompt_template").and_then(|v| v.as_str()).unwrap_or("Give a smart tip about {topic}.").to_string();
    let llm_api_key = settings.get("llm_api_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let llm_base_url = settings.get("llm_base_url").and_then(|v| v.as_str()).unwrap_or("https://openrouter.ai/api/v1").to_string();

    for topic_name in topics.into_iter().take(query.count as usize) {
        let topic_name = topic_name.trim();
        if topic_name.is_empty() { continue; }

        let topic_id: i64 = match sqlx::query_scalar!("SELECT id FROM topics WHERE name = ?", topic_name)
            .fetch_optional(&state.db).await.unwrap_or_default() {
            Some(id) => id.unwrap_or_default(),
            None => {
                sqlx::query!("INSERT INTO topics (name) VALUES (?)", topic_name)
                    .execute(&state.db).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
                    .last_insert_rowid()
            }
        };

        let now = Utc::now();
        let due_card = sqlx::query!(
            r#"
            SELECT t.id, t.full_content, t.compressed_content 
            FROM tipcards t
            JOIN review_states r ON t.id = r.card_id
            WHERE t.topic_id = ? AND r.next_review_at <= ?
            ORDER BY r.next_review_at ASC LIMIT 1
            "#,
            topic_id, now
        ).fetch_optional(&state.db).await.map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some(card) = due_card {
            responses.push(pb::TipCardResponse {
                id: card.id,
                topic: topic_name.to_string(),
                full_content: card.full_content,
                compressed_content: card.compressed_content,
            });
        } else {
            let full_tip = llm::generate_new_card(topic_name, &llm_model, &template, &llm_api_key, &llm_base_url).await;
            let compressed_tip = llm::compress_card(&full_tip, &llm_api_key, &llm_base_url).await;

            let card_id = sqlx::query!(
                "INSERT INTO tipcards (topic_id, full_content, compressed_content) VALUES (?, ?, ?)",
                topic_id, full_tip, compressed_tip
            ).execute(&state.db).await.map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
             .last_insert_rowid();

            let init_state = SrsState::default();
            let state_json = serde_json::to_string(&init_state).unwrap();
            let algo = match init_state.algorithm { Algorithm::SM2 => "sm2", Algorithm::FSRS => "fsrs" };
            
            let now = Utc::now();
            sqlx::query!(
                "INSERT INTO review_states (card_id, algorithm_used, state_data, next_review_at) VALUES (?, ?, ?, ?)",
                card_id, algo, state_json, now
            ).execute(&state.db).await.map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            responses.push(pb::TipCardResponse {
                id: card_id,
                topic: topic_name.to_string(),
                full_content: full_tip,
                compressed_content: compressed_tip,
            });
        }
    }

    let tips_response = pb::TipsResponse { tips: responses };
    Ok(protobuf_response(&tips_response))
}

pub async fn review_card(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    let payload = pb::ReviewPayload::decode(body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let row = sqlx::query!(
        "SELECT state_data FROM review_states WHERE card_id = ?",
        payload.card_id
    ).fetch_optional(&state.db).await.map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = row {
        let mut srs_state: SrsState = serde_json::from_str(&row.state_data).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Invalid state data".into()))?;
        let next_review = srs::calculate_next_review(&mut srs_state, payload.grade as u8);
        let new_state_json = serde_json::to_string(&srs_state).unwrap();

        sqlx::query!(
            "UPDATE review_states SET state_data = ?, next_review_at = ? WHERE card_id = ?",
            new_state_json, next_review, payload.card_id
        ).execute(&state.db).await.map_err(|e: sqlx::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(StatusCode::OK)
    } else {
        Err((StatusCode::NOT_FOUND, "Card not found in user reviews".to_string()))
    }
}
