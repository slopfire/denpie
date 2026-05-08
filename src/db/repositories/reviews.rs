use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct ReviewStateRecord {
    pub state_data: String,
    pub tipcard_type: String,
}

pub async fn load_for_card(pool: &SqlitePool, card_id: i64) -> AppResult<ReviewStateRecord> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT r.state_data, t.tipcard_type
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE r.card_id = ?",
    )
    .bind(card_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Card not found in user reviews".to_string()))?;

    Ok(ReviewStateRecord {
        state_data: row.0,
        tipcard_type: row.1,
    })
}

pub async fn update_queue_state(
    pool: &SqlitePool,
    card_id: i64,
    state_data: String,
    status: String,
    next_review_at: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE review_states SET state_data = ?, status = ?, next_review_at = ? WHERE card_id = ?",
    )
    .bind(state_data)
    .bind(status)
    .bind(next_review_at)
    .bind(card_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_srs_state(
    pool: &SqlitePool,
    card_id: i64,
    state_data: String,
    next_review_at: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query("UPDATE review_states SET state_data = ?, next_review_at = ? WHERE card_id = ?")
        .bind(state_data)
        .bind(next_review_at)
        .bind(card_id)
        .execute(pool)
        .await?;
    Ok(())
}
