use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};

use crate::error::AppResult;

use super::{models::FlowCardRecord, queries, topic_color_from_row};

pub async fn list_flow_cards(
    pool: &PgPool,
    user_id: &str,
    cursor: Option<(i64, String, i64)>,
    limit: i64,
) -> AppResult<Vec<FlowCardRecord>> {
    let limit = limit.clamp(1, 100);
    let now = Utc::now();

    let base = format!(
        "{},
       (
           SELECT COUNT(*)
           FROM review_states pending_review
           JOIN tipcards pending_card ON pending_card.id = pending_review.card_id
           WHERE pending_card.user_id = t.user_id
             AND pending_card.topic_id = t.topic_id
             AND pending_card.tipcard_type = 'repeatable_tip'
             AND pending_review.status = 'pending'
       ) AS pending_count
{} WHERE t.user_id = ",
        queries::BASE_CARD_SELECT,
        queries::FLOW_FROM_JOINS
    );
    let mut builder = QueryBuilder::<Postgres>::new(&base);

    builder.push_bind(user_id);
    builder.push(
        " AND COALESCE(r.status, CASE WHEN top.tipcard_type = 'custom_tip' THEN 'custom' ELSE 'active' END) = 'active'
          AND (t.pinned = 1 OR r.next_review_at IS NULL OR r.next_review_at <= ",
    );
    builder.push_bind(now);
    builder.push(")");
    builder.push(
        " AND (top.tipcard_type != 'repeatable_tip' OR t.id = (
            SELECT t2.id
            FROM tipcards t2
            JOIN review_states r2 ON r2.card_id = t2.id
            WHERE t2.user_id = t.user_id
              AND t2.topic_id = t.topic_id
              AND t2.tipcard_type = 'repeatable_tip'
              AND r2.status = 'active'
              AND (t2.pinned = 1 OR r2.next_review_at <= ",
    );
    builder.push_bind(now);
    builder.push(
        ")
            ORDER BY t2.pinned DESC,
                     CASE WHEN COALESCE(r2.repeats, 0) > 0 THEN 0 ELSE 1 END ASC,
                     r2.next_review_at ASC, t2.created_at ASC, t2.id ASC
            LIMIT 1
        ))",
    );

    if let Some((pinned, created_at, id)) = cursor {
        builder.push(" AND (t.pinned < ");
        builder.push_bind(pinned);
        builder.push(" OR (t.pinned = ");
        builder.push_bind(pinned);
        builder.push(" AND (t.created_at < CAST(");
        builder.push_bind(created_at.clone());
        builder.push(" AS TIMESTAMPTZ) OR (t.created_at = CAST(");
        builder.push_bind(created_at);
        builder.push(" AS TIMESTAMPTZ) AND t.id < ");
        builder.push_bind(id);
        builder.push("))))");
    }

    builder.push(
        " ORDER BY t.pinned DESC, t.created_at DESC, t.id DESC
          LIMIT ",
    );
    builder.push_bind(limit);

    let rows = builder
        .build_query_as::<(
            i64,
            String,
            String,
            Option<i64>,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            i64,
            i64,
        )>()
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| FlowCardRecord {
            id: row.0,
            topic_name: row.1.clone(),
            topic_icon: row.2,
            topic_color: topic_color_from_row(&row.1, row.3),
            title: row.4,
            full_content: row.5,
            compressed_content: row.6,
            created_at: row.7,
            tipcard_type: row.8,
            status: row.9,
            next_review_at: row.10,
            state_data: row.11,
            pinned: row.13 != 0,
            repeats: row.12 as u32,
            pending_count: row.14,
        })
        .collect())
}
