use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::AppResult;

pub async fn last_window_start(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
) -> AppResult<Option<DateTime<Utc>>> {
    Ok(sqlx::query_scalar::<_, DateTime<Utc>>(
        "SELECT window_start
         FROM daily_refresh_runs
         WHERE user_id = $1 AND topic_id = $2 AND tipcard_type = $3",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .fetch_optional(pool)
    .await?)
}

pub async fn mark_window_refreshed(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    window_start: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO daily_refresh_runs (user_id, topic_id, tipcard_type, window_start, refreshed_at)
         VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP)
         ON CONFLICT(user_id, topic_id, tipcard_type)
         DO UPDATE SET window_start = excluded.window_start, refreshed_at = CURRENT_TIMESTAMP",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .bind(window_start)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear_window_refreshed(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    window_start: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM daily_refresh_runs
         WHERE user_id = $1 AND topic_id = $2 AND tipcard_type = $3 AND window_start = $4",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .bind(window_start)
    .execute(pool)
    .await?;
    Ok(())
}
