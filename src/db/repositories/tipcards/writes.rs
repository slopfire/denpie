use chrono::Utc;
use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    domain::review::RepeatableState,
    error::{AppError, AppResult},
};

use super::models::{CreateManualParams, GeneratedCardParams};

pub async fn delete_with_review(pool: &PgPool, user_id: &str, id: i64) -> AppResult<Vec<String>> {
    let mut tx = pool.begin().await?;
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM tipcards WHERE id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("Tipcard not found".to_string()));
    }

    let image_paths = sqlx::query_scalar::<_, String>(
        "DELETE FROM tipcard_images
         WHERE card_id = $1 AND user_id = $2
         RETURNING storage_path",
    )
    .bind(id)
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;

    // review_states is removed by the card foreign key cascade.
    sqlx::query("DELETE FROM tipcards WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(image_paths)
}

/// Delete every generated card whose content is a known generation failure
/// placeholder ("Failed parsing text", "LLM Error: ...", or empty). User-authored
/// cards (manual/custom) are never touched. Returns the number of cards removed
/// and the storage paths whose files should be cleaned up after commit.
pub async fn delete_failed_generation_cards(
    pool: &PgPool,
    user_id: &str,
) -> AppResult<(i64, Vec<String>)> {
    let mut tx = pool.begin().await?;

    let target = "SELECT id FROM tipcards
                  WHERE user_id = $1
                    AND tipcard_type IN ('casual_tip', 'repeatable_tip')
                    AND (TRIM(full_content) = 'Failed parsing text'
                         OR TRIM(full_content) LIKE 'LLM Error:%'
                         OR TRIM(compressed_content) = 'Failed parsing text'
                         OR TRIM(compressed_content) LIKE 'LLM Error:%'
                         OR TRIM(full_content) = '')";

    sqlx::query(&format!(
        "DELETE FROM review_states WHERE card_id IN ({target})"
    ))
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    let image_paths = sqlx::query_scalar::<_, String>(&format!(
        "DELETE FROM tipcard_images WHERE user_id = $1 AND card_id IN ({target}) RETURNING storage_path"
    ))
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;

    let result = sqlx::query(&format!("DELETE FROM tipcards WHERE id IN ({target})"))
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok((result.rows_affected() as i64, image_paths))
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

/// Move a repeatable topic slot's pin from the reviewed physical card to the
/// promoted card. Both updates share the review transaction, so a concurrent
/// flow refresh can never observe the slot temporarily unpinned or doubly pinned.
pub async fn transfer_pinned_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    reviewed_card_id: i64,
    next_card_id: i64,
    pinned: bool,
) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE tipcards
         SET pinned = CASE WHEN id = $1 THEN $2 ELSE 0 END
         WHERE user_id = $3 AND id IN ($1, $4)",
    )
    .bind(next_card_id)
    .bind(if pinned { 1 } else { 0 })
    .bind(user_id)
    .bind(reviewed_card_id)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 2 {
        return Err(AppError::NotFound(
            "Repeatable card slot changed during review".to_string(),
        ));
    }
    Ok(())
}

/// Create a generated card with an explicit review status. For `"pending"` the
/// review state is dated far in the future so the card is never "due" until it is
/// promoted to `"active"` at serve time.
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
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
    let mut tx = pool.begin().await?;
    let card_id = insert_generated_with_status(
        &mut tx,
        user_id,
        topic_id,
        tipcard_type,
        GeneratedCardParams {
            title,
            full_content,
            compressed_content,
            use_image,
            image_query,
        },
        status,
    )
    .await?;
    tx.commit().await?;
    Ok(card_id)
}

/// Persist a generated pending batch once per topic queue state. The topic row
/// lock makes the final pending-count check and all inserts atomic across
/// requests and application instances, without holding a database lock during
/// the external LLM call.
pub async fn create_pending_batch_if_needed(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    low_water: i64,
    cards: &[GeneratedCardParams<'_>],
) -> AppResult<Vec<i64>> {
    if cards.is_empty() {
        return Ok(Vec::new());
    }

    let mut tx = pool.begin().await?;
    let stored_tipcard_type = sqlx::query_scalar::<_, String>(
        "SELECT tipcard_type FROM topics WHERE id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(topic_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(stored_tipcard_type) = stored_tipcard_type else {
        return Err(AppError::NotFound("Topic not found".to_string()));
    };
    if stored_tipcard_type != tipcard_type {
        return Err(AppError::Validation(
            "Card type must match the topic type".to_string(),
        ));
    }

    let pending = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE t.user_id = $1 AND t.topic_id = $2 AND t.tipcard_type = $3
           AND r.status = 'pending'",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .fetch_one(&mut *tx)
    .await?;
    if pending > low_water {
        tx.commit().await?;
        return Ok(Vec::new());
    }

    let mut ids = Vec::with_capacity(cards.len());
    for card in cards {
        ids.push(
            insert_generated_with_status(
                &mut tx,
                user_id,
                topic_id,
                tipcard_type,
                GeneratedCardParams {
                    title: card.title,
                    full_content: card.full_content,
                    compressed_content: card.compressed_content,
                    use_image: card.use_image,
                    image_query: card.image_query,
                },
                "pending",
            )
            .await?,
        );
    }
    tx.commit().await?;
    Ok(ids)
}

async fn insert_generated_with_status(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    card: GeneratedCardParams<'_>,
    status: &str,
) -> AppResult<i64> {
    let card_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content, use_image, image_query)
         SELECT $1, top.id, $3, $4, $5, $6, $7, $8
         FROM topics top
         WHERE top.id = $2 AND top.user_id = $1 AND top.tipcard_type = $3
         RETURNING id",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .bind(card.title)
    .bind(card.full_content)
    .bind(card.compressed_content)
    .bind(if card.use_image { 1 } else { 0 })
    .bind(card.image_query)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Matching topic not found".to_string()))?;

    let state = RepeatableState::default();
    let next_review_at = if status == "pending" {
        Utc::now() + chrono::Duration::days(36500)
    } else {
        Utc::now()
    };
    create_review_state(
        tx,
        card_id,
        state.scheduling_state.algorithm.storage_name(),
        serde_json::to_string(&state)?,
        state.repeats,
        status,
        next_review_at,
    )
    .await?;
    if card.use_image && !card.image_query.trim().is_empty() {
        crate::db::repositories::image_jobs::enqueue_in_tx(tx, user_id, card_id).await?;
    }
    Ok(card_id)
}

pub async fn create_manual(pool: &PgPool, params: CreateManualParams<'_>) -> AppResult<i64> {
    let mut tx = pool.begin().await?;
    let card_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content, image_data)
         SELECT $1, top.id, $3, $4, $5, $6, $7
         FROM topics top
         WHERE top.id = $2 AND top.user_id = $1 AND top.tipcard_type = $3
         RETURNING id",
    )
    .bind(params.user_id)
    .bind(params.topic_id)
    .bind(params.tipcard_type)
    .bind(params.title)
    .bind(params.full_content)
    .bind(params.compressed_content)
    .bind(params.image_data_json)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Matching topic not found".to_string()))?;

    let state = RepeatableState::default();
    create_review_state(
        &mut tx,
        card_id,
        state.scheduling_state.algorithm.storage_name(),
        serde_json::to_string(&state)?,
        state.repeats,
        "active",
        Utc::now(),
    )
    .await?;
    tx.commit().await?;
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
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content)
         SELECT $1, top.id, 'custom_tip', $3, $4, $5
         FROM topics top
         WHERE top.id = $2 AND top.user_id = $1 AND top.tipcard_type = 'custom_tip'
         RETURNING id",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(title)
    .bind(full_content)
    .bind(compressed_content)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Matching topic not found".to_string()))
}

async fn create_review_state(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
    .execute(&mut **tx)
    .await?;
    Ok(())
}
