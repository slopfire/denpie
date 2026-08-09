use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};

use crate::error::AppResult;

use super::{models::ScheduledCardRecord, queries};

pub async fn find_daily_topic_cards(
    pool: &PgPool,
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
    let mut daily_query = QueryBuilder::<Postgres>::new(&base);
    daily_query.push_bind(user_id);
    daily_query.push(" AND t.topic_id = ");
    daily_query.push_bind(topic_id);
    daily_query.push(" AND t.tipcard_type = ");
    daily_query.push_bind(tipcard_type);
    daily_query.push(" AND r.status = 'active'");
    if tipcard_type == "repeatable_tip" {
        daily_query.push(" AND r.reviewed_at IS NULL");
    }
    push_personalized_freshness(&mut daily_query, user_id, topic_id, tipcard_type);
    daily_query.push(" AND (r.daily_refreshed_at IS NULL OR r.daily_refreshed_at < ");
    daily_query.push_bind(daily_window_start);
    daily_query.push(")");
    push_exclusions(&mut daily_query, exclude_card_ids);
    daily_query.push(" ORDER BY t.pinned DESC, t.created_at ASC LIMIT ");
    daily_query.push_bind(limit as i64);

    card_rows(pool, daily_query).await
}

pub async fn find_due_topic_cards(
    pool: &PgPool,
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
    let mut due_query = QueryBuilder::<Postgres>::new(&base);
    due_query.push_bind(user_id);
    due_query.push(" AND t.topic_id = ");
    due_query.push_bind(topic_id);
    due_query.push(" AND t.tipcard_type = ");
    due_query.push_bind(tipcard_type);
    due_query.push(" AND r.status = 'active'");
    push_personalized_freshness(&mut due_query, user_id, topic_id, tipcard_type);
    due_query.push(" AND (r.next_review_at <= ");
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

pub async fn active_card_count(pool: &PgPool, user_id: &str) -> AppResult<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE t.user_id = $1 AND r.status = 'active'
           AND (t.tipcard_type != 'repeatable_tip' OR r.next_review_at <= $2)",
    )
    .bind(user_id)
    .bind(Utc::now())
    .fetch_one(pool)
    .await?)
}

/// Count distinct repeatable cards reviewed in a topic's current daily window.
///
/// `reviewed_at` is updated by every repeatable review, so this measures cards
/// that have actually been worked through rather than the generated backlog.
pub async fn count_reviewed_in_window(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    daily_window_start: DateTime<Utc>,
) -> AppResult<usize> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE t.user_id = $1
           AND t.topic_id = $2
           AND t.tipcard_type = 'repeatable_tip'
           AND r.reviewed_at >= $3",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(daily_window_start)
    .fetch_one(pool)
    .await?;

    Ok(count.max(0) as usize)
}

pub async fn has_active_topic_card(pool: &PgPool, user_id: &str, topic_id: i64) -> AppResult<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM review_states r
            JOIN tipcards t ON t.id = r.card_id
            WHERE t.user_id = $1 AND t.topic_id = $2 AND r.status = 'active'
        )",
    )
    .bind(user_id)
    .bind(topic_id)
    .fetch_one(pool)
    .await?)
}

fn push_exclusions<'args>(
    builder: &mut QueryBuilder<'args, Postgres>,
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

fn push_personalized_freshness<'args>(
    builder: &mut QueryBuilder<'args, Postgres>,
    user_id: &'args str,
    topic_id: i64,
    tipcard_type: &'args str,
) {
    if tipcard_type != "repeatable_tip" {
        return;
    }
    builder.push(
        " AND (t.pinned = 1 OR COALESCE(r.repeats, 0) > 0 OR t.created_at >= COALESCE((
            SELECT MAX(r2.reviewed_at)
            FROM review_states r2
            JOIN tipcards t2 ON t2.id = r2.card_id
            WHERE t2.user_id = ",
    );
    builder.push_bind(user_id);
    builder.push(" AND t2.topic_id = ");
    builder.push_bind(topic_id);
    builder.push(" AND t2.tipcard_type = ");
    builder.push_bind(tipcard_type);
    builder.push(
        " AND r2.feedback IN ('known', 'not_interested', 'too_difficult')),
          TIMESTAMPTZ '-infinity'))",
    );
}

async fn card_rows(
    pool: &PgPool,
    mut query: QueryBuilder<'_, Postgres>,
) -> AppResult<Vec<ScheduledCardRecord>> {
    let rows = query
        .build_query_as::<(i64, String, String, String, i64, String, i64, String)>()
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| ScheduledCardRecord {
            id: row.0,
            full_content: row.1,
            compressed_content: row.2,
            title: row.3,
            use_image: row.4 != 0,
            image_query: row.5,
            pinned: row.6 != 0,
            image_data: row.7,
        })
        .collect())
}
