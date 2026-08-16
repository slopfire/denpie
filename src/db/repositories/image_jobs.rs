use chrono::{Duration, Utc};
use sqlx::{PgPool, Postgres, Transaction};

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ImageJobClaim {
    pub card_id: i64,
    pub user_id: String,
    pub attempts: i64,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ImageJobCard {
    pub topic_id: i64,
    pub topic_name: String,
    pub title: String,
    pub full_content: String,
    pub use_image: bool,
    pub image_query: String,
    pub image_strategy: Option<String>,
    pub review_status: String,
}

pub async fn enqueue_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    card_id: i64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO card_image_jobs (card_id, user_id)
         VALUES ($1, $2)
         ON CONFLICT (card_id) DO UPDATE
         SET status = CASE
                 WHEN card_image_jobs.status = 'processing'
                      AND card_image_jobs.lease_until > CURRENT_TIMESTAMP
                 THEN card_image_jobs.status
                 ELSE 'pending'
             END,
             attempts = CASE
                 WHEN card_image_jobs.status = 'processing'
                      AND card_image_jobs.lease_until > CURRENT_TIMESTAMP
                 THEN card_image_jobs.attempts
                 ELSE 0
             END,
             available_at = CASE
                 WHEN card_image_jobs.status = 'processing'
                      AND card_image_jobs.lease_until > CURRENT_TIMESTAMP
                 THEN card_image_jobs.available_at
                 ELSE CURRENT_TIMESTAMP
             END,
             lease_until = CASE
                 WHEN card_image_jobs.status = 'processing'
                      AND card_image_jobs.lease_until > CURRENT_TIMESTAMP
                 THEN card_image_jobs.lease_until
                 ELSE NULL
             END,
             last_error = CASE
                 WHEN card_image_jobs.status = 'processing'
                      AND card_image_jobs.lease_until > CURRENT_TIMESTAMP
                 THEN card_image_jobs.last_error
                 ELSE ''
             END,
             updated_at = CURRENT_TIMESTAMP",
    )
    .bind(card_id)
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn claim_next(pool: &PgPool) -> AppResult<Option<ImageJobClaim>> {
    let row = sqlx::query_as::<_, ImageJobClaim>(
        "WITH candidate AS (
             SELECT card_id
             FROM card_image_jobs
             WHERE (status = 'pending' AND available_at <= CURRENT_TIMESTAMP)
                OR (status = 'processing' AND lease_until <= CURRENT_TIMESTAMP)
             ORDER BY available_at ASC, created_at ASC, card_id ASC
             LIMIT 1
             FOR UPDATE SKIP LOCKED
         )
         UPDATE card_image_jobs job
         SET status = 'processing',
             attempts = attempts + 1,
             lease_until = CURRENT_TIMESTAMP + INTERVAL '5 minutes',
             updated_at = CURRENT_TIMESTAMP
         FROM candidate
         WHERE job.card_id = candidate.card_id
         RETURNING job.card_id, job.user_id, job.attempts",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn load_card(pool: &PgPool, card_id: i64, user_id: &str) -> AppResult<ImageJobCard> {
    sqlx::query_as::<_, ImageJobCard>(
        "SELECT card.topic_id,
                topic.name AS topic_name,
                COALESCE(card.title, '') AS title,
                card.full_content,
                card.use_image != 0 AS use_image, -- BIGINT 0/1, not boolean
                card.image_query,
                topic.image_strategy,
                COALESCE(review.status, 'active') AS review_status
         FROM tipcards card
         JOIN topics topic ON topic.id = card.topic_id AND topic.user_id = card.user_id
         LEFT JOIN review_states review ON review.card_id = card.id
         WHERE card.id = $1 AND card.user_id = $2",
    )
    .bind(card_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Image enrichment card not found".to_string()))
}

pub async fn mark_completed(pool: &PgPool, card_id: i64) -> AppResult<()> {
    sqlx::query(
        "UPDATE card_image_jobs
         SET status = 'completed', lease_until = NULL, last_error = '', updated_at = CURRENT_TIMESTAMP
         WHERE card_id = $1",
    )
    .bind(card_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_retry_or_failed(
    pool: &PgPool,
    claim: &ImageJobClaim,
    error: &str,
    max_attempts: i64,
) -> AppResult<()> {
    let failed = claim.attempts >= max_attempts;
    let delay_seconds = match claim.attempts {
        0 | 1 => 5,
        2 => 30,
        _ => 120,
    };
    let available_at = Utc::now() + Duration::seconds(delay_seconds);
    sqlx::query(
        "UPDATE card_image_jobs
         SET status = $2,
             available_at = $3,
             lease_until = NULL,
             last_error = $4,
             updated_at = CURRENT_TIMESTAMP
         WHERE card_id = $1 AND status = 'processing'",
    )
    .bind(claim.card_id)
    .bind(if failed { "failed" } else { "pending" })
    .bind(available_at)
    .bind(error.chars().take(500).collect::<String>())
    .execute(pool)
    .await?;
    Ok(())
}

/// Re-run failed jobs and completed jobs that never attached an image.
/// Cards that already have attachments stay completed; the worker would
/// skip them anyway, but leaving them out avoids a no-op claim burst.
pub async fn requeue_failed_for_user(pool: &PgPool, user_id: &str) -> AppResult<u64> {
    let result = sqlx::query(
        "UPDATE card_image_jobs
         SET status = 'pending', attempts = 0, available_at = CURRENT_TIMESTAMP,
             lease_until = NULL, last_error = '', updated_at = CURRENT_TIMESTAMP
         WHERE user_id = $1
           AND (
               status = 'failed'
               OR (
                   status = 'completed'
                   AND NOT EXISTS (
                       SELECT 1
                       FROM tipcard_images img
                       WHERE img.card_id = card_image_jobs.card_id
                         AND img.user_id = $1
                   )
               )
           )",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
