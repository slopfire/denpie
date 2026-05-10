use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::error::AppResult;

pub async fn last_window_start(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT window_start
         FROM daily_refresh_runs
         WHERE user_id = ? AND topic_id = ? AND tipcard_type = ?",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .fetch_optional(pool)
    .await?)
}

pub async fn mark_window_refreshed(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    window_start: DateTime<Utc>,
) -> AppResult<()> {
    let window_start = window_start
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    sqlx::query(
        "INSERT INTO daily_refresh_runs (user_id, topic_id, tipcard_type, window_start, refreshed_at)
         VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)
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
