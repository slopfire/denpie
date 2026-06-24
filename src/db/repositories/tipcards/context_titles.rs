use sqlx::SqlitePool;

use crate::error::AppResult;

use super::models::CardContextTitleRecord;

pub async fn list_context_titles(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    limit: i64,
) -> AppResult<Vec<CardContextTitleRecord>> {
    let rows = sqlx::query_as::<_, CardContextTitleRecord>(
        "SELECT COALESCE(NULLIF(t.title, ''), t.compressed_content) AS title,
                COALESCE(r.status, 'active') AS status
         FROM tipcards t
         LEFT JOIN review_states r ON r.card_id = t.id
         WHERE t.user_id = ? AND t.topic_id = ? AND t.tipcard_type = ?
         ORDER BY t.created_at DESC, t.id DESC
         LIMIT ?",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}
