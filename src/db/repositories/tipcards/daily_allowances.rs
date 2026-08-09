use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::AppResult;

pub async fn extra_cards_in_window(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    window_start: DateTime<Utc>,
) -> AppResult<usize> {
    let extra_cards = sqlx::query_scalar::<_, i64>(
        "SELECT extra_cards
         FROM repeatable_daily_allowances
         WHERE user_id = $1 AND topic_id = $2 AND tipcard_type = $3 AND window_start = $4",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .bind(window_start)
    .fetch_optional(pool)
    .await?
    .unwrap_or_default();

    Ok(extra_cards.max(0) as usize)
}

pub async fn add_extra_cards(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    window_start: DateTime<Utc>,
    extra_cards: usize,
) -> AppResult<()> {
    if extra_cards == 0 {
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO repeatable_daily_allowances
             (user_id, topic_id, tipcard_type, window_start, extra_cards)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_id, topic_id, tipcard_type, window_start)
         DO UPDATE SET extra_cards = repeatable_daily_allowances.extra_cards + EXCLUDED.extra_cards,
                       updated_at = CURRENT_TIMESTAMP",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .bind(window_start)
    .bind(extra_cards as i64)
    .execute(pool)
    .await?;

    Ok(())
}
