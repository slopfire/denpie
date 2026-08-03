use chrono::Utc;
use sqlx::SqlitePool;

use crate::error::AppResult;

use super::{models::ScheduledCardRecord, queries};

/// Keep one due repeatable card active per topic and move the rest behind it.
pub async fn stack_due_repeatable_cards(pool: &SqlitePool, user_id: &str) -> AppResult<()> {
    sqlx::query(
        "WITH ranked AS (
            SELECT r.card_id, COALESCE(r.repeats, 0) AS repeats,
                   ROW_NUMBER() OVER (
                       PARTITION BY t.topic_id
                       ORDER BY t.pinned DESC,
                                CASE WHEN COALESCE(r.repeats, 0) > 0 THEN 0 ELSE 1 END,
                                r.next_review_at ASC, t.created_at ASC, t.id ASC
                   ) AS position
            FROM review_states r
            JOIN tipcards t ON t.id = r.card_id
            WHERE t.user_id = ?
              AND t.tipcard_type = 'repeatable_tip'
              AND r.status = 'active'
              AND r.next_review_at <= ?
        )
        UPDATE review_states
        SET status = 'pending'
        WHERE card_id IN (
            SELECT card_id FROM ranked WHERE position > 1 AND repeats = 0
        )",
    )
    .bind(user_id)
    .bind(Utc::now())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn park_unseen_active_topic_cards(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE review_states
         SET status = 'pending'
         WHERE status = 'active' AND repeats = 0
           AND card_id IN (
               SELECT id FROM tipcards
               WHERE user_id = ? AND topic_id = ? AND tipcard_type = 'repeatable_tip'
           )",
    )
    .bind(user_id)
    .bind(topic_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn promote_pending_for_empty_topics(pool: &SqlitePool, user_id: &str) -> AppResult<()> {
    let now = Utc::now();
    sqlx::query(
        "WITH candidates AS (
            SELECT r.card_id,
                   ROW_NUMBER() OVER (
                       PARTITION BY t.topic_id
                       ORDER BY t.created_at ASC, t.id ASC
                   ) AS position
            FROM review_states r
            JOIN tipcards t ON t.id = r.card_id
            WHERE t.user_id = ?
              AND t.tipcard_type = 'repeatable_tip'
              AND r.status = 'pending'
              AND t.created_at >= COALESCE((
                  SELECT MAX(r3.reviewed_at)
                  FROM review_states r3
                  JOIN tipcards t3 ON t3.id = r3.card_id
                  WHERE t3.user_id = t.user_id
                    AND t3.topic_id = t.topic_id
                    AND r3.feedback IN ('known', 'not_interested', 'too_difficult')
              ), '0000-01-01 00:00:00')
              AND NOT EXISTS (
                  SELECT 1
                  FROM review_states r2
                  JOIN tipcards t2 ON t2.id = r2.card_id
                  WHERE t2.user_id = t.user_id
                    AND t2.topic_id = t.topic_id
                    AND r2.status = 'active'
                    AND (r2.next_review_at <= ? OR t2.pinned = 1)
              )
        )
        UPDATE review_states
        SET status = 'active', next_review_at = ?
        WHERE card_id IN (
            SELECT card_id FROM candidates WHERE position = 1
        )",
    )
    .bind(user_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

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
           AND (? != 'repeatable_tip' OR t.created_at >= COALESCE((
               SELECT MAX(r2.reviewed_at)
               FROM review_states r2
               JOIN tipcards t2 ON t2.id = r2.card_id
               WHERE t2.user_id = ? AND t2.topic_id = ? AND t2.tipcard_type = ?
                 AND r2.feedback IN ('known', 'not_interested', 'too_difficult')
           ), '0000-01-01 00:00:00'))
         ORDER BY t.created_at ASC, t.id ASC
         LIMIT 1",
            select = queries::SCHEDULED_SELECT
        ))
        .bind(user_id)
        .bind(topic_id)
        .bind(tipcard_type)
        .bind(tipcard_type)
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

/// Replace an unseen active repeatable card with the oldest card that was
/// already pending. The candidate is selected before the active card is parked,
/// so a forced topic load cannot immediately select the same card again.
pub async fn replace_unseen_with_pending_card(
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
           AND (? != 'repeatable_tip' OR t.created_at >= COALESCE((
               SELECT MAX(r2.reviewed_at)
               FROM review_states r2
               JOIN tipcards t2 ON t2.id = r2.card_id
               WHERE t2.user_id = ? AND t2.topic_id = ? AND t2.tipcard_type = ?
                 AND r2.feedback IN ('known', 'not_interested', 'too_difficult')
           ), '0000-01-01 00:00:00'))
         ORDER BY t.created_at ASC, t.id ASC
         LIMIT 1",
            select = queries::SCHEDULED_SELECT
        ))
        .bind(user_id)
        .bind(topic_id)
        .bind(tipcard_type)
        .bind(tipcard_type)
        .bind(user_id)
        .bind(topic_id)
        .bind(tipcard_type)
        .fetch_optional(&mut *tx)
        .await?;

    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };

    if tipcard_type == "repeatable_tip" {
        sqlx::query(
            "UPDATE review_states
             SET status = 'pending'
             WHERE status = 'active' AND repeats = 0 AND card_id != ?
               AND card_id IN (
                   SELECT id FROM tipcards
                   WHERE user_id = ? AND topic_id = ? AND tipcard_type = 'repeatable_tip'
               )",
        )
        .bind(row.0)
        .bind(user_id)
        .bind(topic_id)
        .execute(&mut *tx)
        .await?;
    }

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
