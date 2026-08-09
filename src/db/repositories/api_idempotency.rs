use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::{AppError, AppResult};

const RETENTION_HOURS: i32 = 24;

#[derive(Debug, PartialEq, Eq)]
pub enum IdempotencyRecord {
    Acquired,
    Completed {
        status_code: u16,
        response_body: Option<Vec<u8>>,
    },
    Conflict,
    InProgress {
        created_at: DateTime<Utc>,
    },
}

pub async fn claim(
    pool: &PgPool,
    actor_id: &str,
    user_id: &str,
    idempotency_key: &str,
    request_hash: &str,
) -> AppResult<IdempotencyRecord> {
    let acquired = sqlx::query_scalar::<_, bool>(
        "WITH expired AS (
             SELECT actor_id, idempotency_key
             FROM api_idempotency_keys
             WHERE state = 'completed' AND expires_at <= CURRENT_TIMESTAMP
             ORDER BY expires_at
             LIMIT 100
         ), cleanup AS (
             DELETE FROM api_idempotency_keys target
             USING expired
             WHERE target.actor_id = expired.actor_id
               AND target.idempotency_key = expired.idempotency_key
         )
         INSERT INTO api_idempotency_keys
             (actor_id, user_id, idempotency_key, request_hash, expires_at)
         VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP + ($5 * INTERVAL '1 hour'))
         ON CONFLICT (actor_id, idempotency_key) DO UPDATE
         SET user_id = EXCLUDED.user_id,
             request_hash = EXCLUDED.request_hash,
             state = 'in_progress',
             status_code = NULL,
             response_body = NULL,
             created_at = CURRENT_TIMESTAMP,
             completed_at = NULL,
             expires_at = EXCLUDED.expires_at
         WHERE api_idempotency_keys.state = 'completed'
           AND api_idempotency_keys.expires_at <= CURRENT_TIMESTAMP
         RETURNING TRUE",
    )
    .bind(actor_id)
    .bind(user_id)
    .bind(idempotency_key)
    .bind(request_hash)
    .bind(RETENTION_HOURS)
    .fetch_optional(pool)
    .await?;

    if acquired.is_some() {
        return Ok(IdempotencyRecord::Acquired);
    }
    lookup(pool, actor_id, idempotency_key, request_hash).await
}

pub async fn lookup(
    pool: &PgPool,
    actor_id: &str,
    idempotency_key: &str,
    request_hash: &str,
) -> AppResult<IdempotencyRecord> {
    let row = sqlx::query_as::<_, (String, String, Option<i32>, Option<Vec<u8>>, DateTime<Utc>)>(
        "SELECT request_hash, state, status_code, response_body, created_at
         FROM api_idempotency_keys
         WHERE actor_id = $1 AND idempotency_key = $2",
    )
    .bind(actor_id)
    .bind(idempotency_key)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::Conflict("Idempotency claim disappeared".to_string()))?;

    if row.0 != request_hash {
        return Ok(IdempotencyRecord::Conflict);
    }
    if row.1 == "in_progress" {
        return Ok(IdempotencyRecord::InProgress { created_at: row.4 });
    }
    let status_code = row
        .2
        .and_then(|status| u16::try_from(status).ok())
        .ok_or_else(|| AppError::Db(sqlx::Error::Protocol("Invalid stored HTTP status".into())))?;
    Ok(IdempotencyRecord::Completed {
        status_code,
        response_body: row.3,
    })
}

pub async fn complete(
    pool: &PgPool,
    actor_id: &str,
    idempotency_key: &str,
    request_hash: &str,
    status_code: u16,
    response_body: Option<&[u8]>,
) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE api_idempotency_keys
         SET state = 'completed',
             status_code = $4,
             response_body = $5,
             completed_at = CURRENT_TIMESTAMP,
             expires_at = CURRENT_TIMESTAMP + ($6 * INTERVAL '1 hour')
         WHERE actor_id = $1
           AND idempotency_key = $2
           AND request_hash = $3
           AND state = 'in_progress'",
    )
    .bind(actor_id)
    .bind(idempotency_key)
    .bind(request_hash)
    .bind(i32::from(status_code))
    .bind(response_body)
    .bind(RETENTION_HOURS)
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict(
            "Idempotency claim could not be completed".to_string(),
        ));
    }
    Ok(())
}
