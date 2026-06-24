use chrono::Utc;
use sqlx::SqlitePool;

use crate::{
    domain::review::RepeatableState,
    error::{AppError, AppResult},
};

use super::models::CreateManualParams;

pub async fn delete_with_review(pool: &SqlitePool, user_id: &str, id: i64) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "DELETE FROM review_states
         WHERE card_id IN (SELECT id FROM tipcards WHERE id = ? AND user_id = ?)",
    )
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM tipcard_images WHERE card_id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    let result = sqlx::query("DELETE FROM tipcards WHERE id = ? AND user_id = ?")
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

pub async fn set_pinned(pool: &SqlitePool, user_id: &str, id: i64, pinned: bool) -> AppResult<()> {
    let result = sqlx::query("UPDATE tipcards SET pinned = ? WHERE id = ? AND user_id = ?")
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

pub async fn create_generated(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    title: &str,
    full_content: &str,
    compressed_content: &str,
) -> AppResult<i64> {
    let card_id = sqlx::query(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .bind(title)
    .bind(full_content)
    .bind(compressed_content)
    .execute(pool)
    .await?
    .last_insert_rowid();

    let state = RepeatableState::default();
    create_review_state(
        pool,
        card_id,
        state.scheduling_state.algorithm.storage_name(),
        serde_json::to_string(&state)?,
        state.repeats,
        Utc::now(),
    )
    .await?;
    Ok(card_id)
}

pub async fn create_manual(pool: &SqlitePool, params: CreateManualParams<'_>) -> AppResult<i64> {
    let card_id = sqlx::query(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content, image_data) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(params.user_id)
    .bind(params.topic_id)
    .bind(params.tipcard_type)
    .bind(params.title)
    .bind(params.full_content)
    .bind(params.compressed_content)
    .bind(params.image_data_json)
    .execute(pool)
    .await?
    .last_insert_rowid();

    let state = RepeatableState::default();
    create_review_state(
        pool,
        card_id,
        state.scheduling_state.algorithm.storage_name(),
        serde_json::to_string(&state)?,
        state.repeats,
        Utc::now(),
    )
    .await?;
    Ok(card_id)
}

pub async fn create_custom(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    title: &str,
    full_content: &str,
    compressed_content: &str,
) -> AppResult<i64> {
    Ok(sqlx::query(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content) VALUES (?, ?, 'custom_tip', ?, ?, ?)",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(title)
    .bind(full_content)
    .bind(compressed_content)
    .execute(pool)
    .await?
    .last_insert_rowid())
}

async fn create_review_state(
    pool: &SqlitePool,
    card_id: i64,
    algorithm_used: &str,
    state_data: String,
    repeats: u32,
    next_review_at: chrono::DateTime<chrono::Utc>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, repeats, status, next_review_at) VALUES (?, ?, ?, ?, 'active', ?)",
    )
    .bind(card_id)
    .bind(algorithm_used)
    .bind(state_data)
    .bind(repeats)
    .bind(next_review_at)
    .execute(pool)
    .await?;
    Ok(())
}
