use chrono::Utc;
use sqlx::PgPool;

use crate::{
    domain::review::RepeatableState,
    error::{AppError, AppResult},
};

use super::models::CreateManualParams;

pub async fn delete_with_review(pool: &PgPool, user_id: &str, id: i64) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "DELETE FROM review_states
         WHERE card_id IN (SELECT id FROM tipcards WHERE id = $1 AND user_id = $2)",
    )
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM tipcard_images WHERE card_id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    let result = sqlx::query("DELETE FROM tipcards WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Tipcard not found".to_string()));
    }

    tx.commit().await?;
    Ok(())
}

pub async fn set_pinned(pool: &PgPool, user_id: &str, id: i64, pinned: bool) -> AppResult<()> {
    let result = sqlx::query("UPDATE tipcards SET pinned = $1 WHERE id = $2 AND user_id = $3")
        .bind(if pinned { 1 } else { 0 })
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Tipcard not found".to_string()));
    }
    Ok(())
}

/// Create a generated card with an explicit review status. For `"pending"` the
/// review state is dated far in the future so the card is never "due" until it is
/// promoted to `"active"` at serve time.
#[allow(clippy::too_many_arguments)]
pub async fn create_generated_with_status(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    title: &str,
    full_content: &str,
    compressed_content: &str,
    use_image: bool,
    image_query: &str,
    status: &str,
) -> AppResult<i64> {
    let card_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content, use_image, image_query)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .bind(title)
    .bind(full_content)
    .bind(compressed_content)
    .bind(if use_image { 1 } else { 0 })
    .bind(image_query)
    .fetch_one(pool)
    .await?;

    let state = RepeatableState::default();
    let next_review_at = if status == "pending" {
        Utc::now() + chrono::Duration::days(36500)
    } else {
        Utc::now()
    };
    create_review_state(
        pool,
        card_id,
        state.scheduling_state.algorithm.storage_name(),
        serde_json::to_string(&state)?,
        state.repeats,
        status,
        next_review_at,
    )
    .await?;
    Ok(card_id)
}

pub async fn create_manual(pool: &PgPool, params: CreateManualParams<'_>) -> AppResult<i64> {
    let card_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content, image_data)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(params.user_id)
    .bind(params.topic_id)
    .bind(params.tipcard_type)
    .bind(params.title)
    .bind(params.full_content)
    .bind(params.compressed_content)
    .bind(params.image_data_json)
    .fetch_one(pool)
    .await?;

    let state = RepeatableState::default();
    create_review_state(
        pool,
        card_id,
        state.scheduling_state.algorithm.storage_name(),
        serde_json::to_string(&state)?,
        state.repeats,
        "active",
        Utc::now(),
    )
    .await?;
    Ok(card_id)
}

pub async fn create_custom(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    title: &str,
    full_content: &str,
    compressed_content: &str,
) -> AppResult<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content)
         VALUES ($1, $2, 'custom_tip', $3, $4, $5)
         RETURNING id",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(title)
    .bind(full_content)
    .bind(compressed_content)
    .fetch_one(pool)
    .await?)
}

async fn create_review_state(
    pool: &PgPool,
    card_id: i64,
    algorithm_used: &str,
    state_data: String,
    repeats: u32,
    status: &str,
    next_review_at: chrono::DateTime<chrono::Utc>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, repeats, status, next_review_at) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(card_id)
    .bind(algorithm_used)
    .bind(state_data)
    .bind(i64::from(repeats))
    .bind(status)
    .bind(next_review_at)
    .execute(pool)
    .await?;
    Ok(())
}
