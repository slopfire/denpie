use sqlx::PgPool;

use crate::error::AppResult;

use super::models::CardContextTitleRecord;

pub async fn list_context_titles(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    limit: i64,
) -> AppResult<Vec<CardContextTitleRecord>> {
    let rows = sqlx::query_as::<_, CardContextTitleRecord>(
        "SELECT COALESCE(NULLIF(t.title, ''), t.compressed_content) AS title,
                COALESCE(r.status, 'active') AS status,
                COALESCE(NULLIF(r.feedback, ''), CASE WHEN r.status = 'dismissed' THEN 'not_interested' ELSE '' END) AS feedback
         FROM tipcards t
         LEFT JOIN review_states r ON r.card_id = t.id
         WHERE t.user_id = $1 AND t.topic_id = $2 AND t.tipcard_type = $3
           AND COALESCE(r.feedback, '') != 'superseded'
         ORDER BY t.created_at DESC, t.id DESC
         LIMIT $4",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Recent titles across one topic, any card type. Used by prompt-template
/// review, which cares about generated-card history rather than one queue.
pub async fn list_history_titles_for_topic(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    limit: i64,
) -> AppResult<Vec<CardContextTitleRecord>> {
    let rows = sqlx::query_as::<_, CardContextTitleRecord>(
        "SELECT COALESCE(NULLIF(t.title, ''), t.compressed_content) AS title,
                COALESCE(r.status, 'active') AS status,
                COALESCE(NULLIF(r.feedback, ''), CASE WHEN r.status = 'dismissed' THEN 'not_interested' ELSE '' END) AS feedback
         FROM tipcards t
         LEFT JOIN review_states r ON r.card_id = t.id
         WHERE t.user_id = $1 AND t.topic_id = $2
           AND COALESCE(r.feedback, '') != 'superseded'
         ORDER BY t.created_at DESC, t.id DESC
         LIMIT $3",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Recent titles across every topic the user owns.
pub async fn list_history_titles_for_user(
    pool: &PgPool,
    user_id: &str,
    limit: i64,
) -> AppResult<Vec<CardContextTitleRecord>> {
    let rows = sqlx::query_as::<_, CardContextTitleRecord>(
        "SELECT COALESCE(NULLIF(t.title, ''), t.compressed_content) AS title,
                COALESCE(r.status, 'active') AS status,
                COALESCE(NULLIF(r.feedback, ''), CASE WHEN r.status = 'dismissed' THEN 'not_interested' ELSE '' END) AS feedback
         FROM tipcards t
         LEFT JOIN review_states r ON r.card_id = t.id
         WHERE t.user_id = $1
           AND COALESCE(r.feedback, '') != 'superseded'
         ORDER BY t.created_at DESC, t.id DESC
         LIMIT $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}
