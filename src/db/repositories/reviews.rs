use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct ReviewStateRecord {
    pub state_data: String,
    pub tipcard_type: String,
    pub repeats: u32,
}

pub struct QueueReviewUpdate<'a> {
    pub state_data: String,
    pub repeats: u32,
    pub status: String,
    pub feedback: &'a str,
    pub next_review_at: DateTime<Utc>,
}

pub async fn load_for_card(
    pool: &PgPool,
    user_id: &str,
    card_id: i64,
) -> AppResult<ReviewStateRecord> {
    let row = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT r.state_data, top.tipcard_type, r.repeats
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         JOIN topics top ON t.topic_id = top.id
         WHERE t.user_id = $1 AND r.card_id = $2",
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
    pool: &PgPool,
    user_id: &str,
    card_id: i64,
    update: QueueReviewUpdate<'_>,
) -> AppResult<()> {
    let reviewed_at = Utc::now();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE review_states
         SET state_data = $1, repeats = $2, status = $3, feedback = $4, reviewed_at = $5, next_review_at = $6
         WHERE card_id IN (SELECT id FROM tipcards WHERE id = $7 AND user_id = $8)",
    )
    .bind(update.state_data)
    .bind(i64::from(update.repeats))
    .bind(update.status)
    .bind(update.feedback)
    .bind(reviewed_at)
    .bind(update.next_review_at)
    .bind(card_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    if matches!(
        update.feedback,
        "known" | "not_interested" | "too_difficult"
    ) {
        sqlx::query(
            "UPDATE review_states
             SET status = 'dismissed', feedback = 'superseded'
             WHERE repeats = 0
               AND (status = 'pending' OR status = 'active')
               AND card_id IN (
                 SELECT stale.id
                 FROM tipcards stale
                 JOIN tipcards reviewed ON reviewed.id = $1 AND reviewed.user_id = $2
                 WHERE stale.user_id = $3
                   AND stale.topic_id = reviewed.topic_id
                   AND stale.tipcard_type = reviewed.tipcard_type
                   AND reviewed.tipcard_type = 'repeatable_tip'
                   AND stale.id != reviewed.id
                   AND stale.pinned = 0
                   AND stale.created_at <= $4
             )",
        )
        .bind(card_id)
        .bind(user_id)
        .bind(user_id)
        .bind(reviewed_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn update_review_schedule(
    pool: &PgPool,
    user_id: &str,
    card_id: i64,
    state_data: String,
    repeats: u32,
    next_review_at: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE review_states
         SET state_data = $1, repeats = $2, next_review_at = $3
         WHERE card_id IN (SELECT id FROM tipcards WHERE id = $4 AND user_id = $5)",
    )
    .bind(state_data)
    .bind(i64::from(repeats))
    .bind(next_review_at)
    .bind(card_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}
