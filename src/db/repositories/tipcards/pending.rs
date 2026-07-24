use chrono::Utc;
use sqlx::SqlitePool;

use crate::error::AppResult;

use super::{models::ScheduledCardRecord, queries};

/// Take the oldest pending card for a topic, flip it to `active` with an
/// immediate `next_review_at`, and return it. Returns `None` when no pending card
/// exists. The select + update run in one transaction so concurrent callers do
/// not promote the same card twice.
pub async fn take_pending_card(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
) -> AppResult<Option<ScheduledCardRecord>> {
    let mut tx = pool.begin().await?;

    let row =
        sqlx::query_as::<_, (i64, String, String, String, i64, String, i64, String)>(&format!(
            "{select} JOIN review_states r ON t.id = r.card_id
         WHERE t.user_id = ? AND t.topic_id = ? AND t.tipcard_type = ? AND r.status = 'pending'
         ORDER BY t.created_at ASC, t.id ASC
         LIMIT 1",
            select = queries::SCHEDULED_SELECT
        ))
        .bind(user_id)
        .bind(topic_id)
        .bind(tipcard_type)
        .fetch_optional(&mut *tx)
        .await?;

    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };

    sqlx::query("UPDATE review_states SET status = 'active', next_review_at = ? WHERE card_id = ?")
        .bind(Utc::now())
        .bind(row.0)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(Some(ScheduledCardRecord {
        id: row.0,
        full_content: row.1,
        compressed_content: row.2,
        title: row.3,
        use_image: row.4 != 0,
        image_query: row.5,
        pinned: row.6 != 0,
        image_data: row.7,
    }))
}

/// Count pending cards for a topic.
#[allow(dead_code)]
pub async fn count_pending(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
) -> AppResult<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE t.user_id = ? AND t.topic_id = ? AND t.tipcard_type = ? AND r.status = 'pending'",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .fetch_one(pool)
    .await?)
}
