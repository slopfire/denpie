use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct ReviewStateRecord {
    pub state_data: String,
    pub tipcard_type: String,
    pub repeats: u32,
}

pub async fn load_for_card(
    pool: &SqlitePool,
    user_id: &str,
    card_id: i64,
) -> AppResult<ReviewStateRecord> {
    let row = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT r.state_data, top.tipcard_type, r.repeats
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         JOIN topics top ON t.topic_id = top.id
         WHERE t.user_id = ? AND r.card_id = ?",
    )
    .bind(user_id)
    .bind(card_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Card not found in user reviews".to_string()))?;

    Ok(ReviewStateRecord {
        state_data: row.0,
        tipcard_type: row.1,
        repeats: row.2 as u32,
    })
}

pub async fn update_queue_state(
    pool: &SqlitePool,
    user_id: &str,
    card_id: i64,
    state_data: String,
    repeats: u32,
    status: String,
    next_review_at: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE review_states
         SET state_data = ?, repeats = ?, status = ?, next_review_at = ?
         WHERE card_id IN (SELECT id FROM tipcards WHERE id = ? AND user_id = ?)",
    )
    .bind(state_data)
    .bind(repeats)
    .bind(status)
    .bind(next_review_at)
    .bind(card_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_review_schedule(
    pool: &SqlitePool,
    user_id: &str,
    card_id: i64,
    state_data: String,
    repeats: u32,
    next_review_at: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE review_states
         SET state_data = ?, repeats = ?, next_review_at = ?
         WHERE card_id IN (SELECT id FROM tipcards WHERE id = ? AND user_id = ?)",
    )
    .bind(state_data)
    .bind(repeats)
    .bind(next_review_at)
    .bind(card_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}
