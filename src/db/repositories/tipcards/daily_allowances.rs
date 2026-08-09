use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};

use crate::error::AppResult;

use super::{models::DailyReviewTarget, pending};

pub async fn promote_pending_within_daily_limits(
    pool: &PgPool,
    user_id: &str,
    targets: &[DailyReviewTarget],
) -> AppResult<()> {
    if targets.is_empty() {
        return Ok(());
    }

    let topic_ids = targets
        .iter()
        .map(|target| target.topic_id)
        .collect::<Vec<_>>();
    let mut tx = pool.begin().await?;

    // Review application and generated-batch persistence take the same topic
    // lock before touching review rows. Locking in ID order makes the daily
    // eligibility check and promotion one serializable per-topic decision.
    sqlx::query_scalar::<_, i64>(
        "SELECT id
         FROM topics
         WHERE user_id = $1 AND id = ANY($2)
         ORDER BY id
         FOR UPDATE",
    )
    .bind(user_id)
    .bind(&topic_ids)
    .fetch_all(&mut *tx)
    .await?;

    let eligible_topic_ids = eligible_repeatable_topic_ids(&mut tx, user_id, targets).await?;
    pending::promote_pending_for_empty_topics_in_tx(&mut tx, user_id, &eligible_topic_ids).await?;
    tx.commit().await?;
    Ok(())
}

async fn eligible_repeatable_topic_ids(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    targets: &[DailyReviewTarget],
) -> AppResult<Vec<i64>> {
    let topic_ids = targets
        .iter()
        .map(|target| target.topic_id)
        .collect::<Vec<_>>();
    let window_starts = targets
        .iter()
        .map(|target| target.window_start)
        .collect::<Vec<_>>();
    let daily_card_counts = targets
        .iter()
        .map(|target| target.daily_card_count)
        .collect::<Vec<_>>();

    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT target.topic_id
         FROM UNNEST($2::BIGINT[], $3::TIMESTAMPTZ[], $4::BIGINT[])
              AS target(topic_id, window_start, daily_card_count)
         WHERE (
             SELECT COUNT(*)
             FROM review_states r
             JOIN tipcards t ON t.id = r.card_id
             WHERE t.user_id = $1
               AND t.topic_id = target.topic_id
               AND t.tipcard_type = 'repeatable_tip'
               AND r.reviewed_at >= target.window_start
         ) < target.daily_card_count + COALESCE((
             SELECT allowance.extra_cards
             FROM repeatable_daily_allowances allowance
             WHERE allowance.user_id = $1
               AND allowance.topic_id = target.topic_id
               AND allowance.tipcard_type = 'repeatable_tip'
               AND allowance.window_start = target.window_start
         ), 0)",
    )
    .bind(user_id)
    .bind(topic_ids)
    .bind(window_starts)
    .bind(daily_card_counts)
    .fetch_all(&mut **tx)
    .await?)
}

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
