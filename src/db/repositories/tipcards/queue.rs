use chrono::{DateTime, Utc};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::error::AppResult;

use super::{models::ScheduledCardRecord, queries};

pub async fn find_daily_topic_cards(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    daily_window_start: DateTime<Utc>,
    exclude_card_ids: &[i64],
    limit: usize,
) -> AppResult<Vec<ScheduledCardRecord>> {
    let base = format!(
        "{} JOIN review_states r ON t.id = r.card_id\n          WHERE t.user_id = ",
        queries::SCHEDULED_SELECT
    );
    let mut daily_query = QueryBuilder::<Sqlite>::new(&base);
    daily_query.push_bind(user_id);
    daily_query.push(" AND t.topic_id = ");
    daily_query.push_bind(topic_id);
    daily_query.push(" AND t.tipcard_type = ");
    daily_query.push_bind(tipcard_type);
    daily_query.push(" AND r.status = 'active'");
    daily_query.push(" AND (r.daily_refreshed_at IS NULL OR r.daily_refreshed_at < ");
    daily_query.push_bind(
        daily_window_start
            .naive_utc()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    );
    daily_query.push(")");
    push_exclusions(&mut daily_query, exclude_card_ids);
    daily_query.push(" ORDER BY t.pinned DESC, t.created_at ASC LIMIT ");
    daily_query.push_bind(limit as i64);

    card_rows(pool, daily_query).await
}

pub async fn find_due_topic_cards(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    exclude_card_ids: &[i64],
    limit: usize,
) -> AppResult<Vec<ScheduledCardRecord>> {
    let now = Utc::now();
    let base = format!(
        "{} JOIN review_states r ON t.id = r.card_id\n          WHERE t.user_id = ",
        queries::SCHEDULED_SELECT
    );
    let mut due_query = QueryBuilder::<Sqlite>::new(&base);
    due_query.push_bind(user_id);
    due_query.push(" AND t.topic_id = ");
    due_query.push_bind(topic_id);
    due_query.push(" AND t.tipcard_type = ");
    due_query.push_bind(tipcard_type);
    due_query.push(" AND r.status = 'active' AND (r.next_review_at <= ");
    due_query.push_bind(now);
    due_query.push(" OR t.pinned = 1)");
    push_exclusions(&mut due_query, exclude_card_ids);
    due_query.push(
        " ORDER BY
            t.pinned DESC,
            CASE
                WHEN t.tipcard_type = 'repeatable_tip'
                     AND COALESCE(r.repeats, 0) > 0
                THEN 0
                ELSE 1
            END ASC,
            r.next_review_at ASC
        LIMIT ",
    );
    due_query.push_bind(limit as i64);

    card_rows(pool, due_query).await
}

pub async fn active_card_count(pool: &SqlitePool, user_id: &str) -> AppResult<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE t.user_id = ? AND r.status = 'active'",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?)
}

fn push_exclusions<'args>(
    builder: &mut QueryBuilder<'args, Sqlite>,
    exclude_card_ids: &'args [i64],
) {
    if !exclude_card_ids.is_empty() {
        builder.push(" AND t.id NOT IN (");
        let mut separated = builder.separated(", ");
        for id in exclude_card_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
    }
}

async fn card_rows(
    pool: &SqlitePool,
    mut query: QueryBuilder<'_, Sqlite>,
) -> AppResult<Vec<ScheduledCardRecord>> {
    let rows = query
        .build_query_as::<(i64, String, String, i64, String)>()
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| ScheduledCardRecord {
            id: row.0,
            full_content: row.1,
            compressed_content: row.2,
            pinned: row.3 != 0,
            image_data: row.4,
        })
        .collect())
}
