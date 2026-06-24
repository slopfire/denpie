use chrono::Utc;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::error::AppResult;

use super::{models::FlowCardRecord, queries, topic_color_from_row};

pub async fn list_flow_cards(
    pool: &SqlitePool,
    user_id: &str,
    cursor: Option<(i64, String, i64)>,
    limit: i64,
) -> AppResult<Vec<FlowCardRecord>> {
    let limit = limit.clamp(1, 100);
    let now = Utc::now();

    let base = format!(
        "{},\n       COUNT(img.id) AS image_count\n{} WHERE t.user_id = ",
        queries::BASE_CARD_SELECT,
        queries::FLOW_FROM_JOINS
    );
    let mut builder = QueryBuilder::<Sqlite>::new(&base);

    builder.push_bind(user_id);
    builder.push(
        " AND COALESCE(r.status, CASE WHEN top.tipcard_type = 'custom_tip' THEN 'custom' ELSE 'active' END) = 'active'
          AND (COALESCE(t.pinned, 0) = 1 OR r.next_review_at IS NULL OR r.next_review_at <= ",
    );
    builder.push_bind(now);
    builder.push(")");

    if let Some((pinned, created_at, id)) = cursor {
        builder.push(" AND (COALESCE(t.pinned, 0) < ");
        builder.push_bind(pinned);
        builder.push(" OR (COALESCE(t.pinned, 0) = ");
        builder.push_bind(pinned);
        builder.push(" AND (CAST(t.created_at AS TEXT) < ");
        builder.push_bind(created_at.clone());
        builder.push(" OR (CAST(t.created_at AS TEXT) = ");
        builder.push_bind(created_at);
        builder.push(" AND t.id < ");
        builder.push_bind(id);
        builder.push("))))");
    }

    builder.push(
        " GROUP BY t.id
          ORDER BY pinned DESC, t.created_at DESC, t.id DESC
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
            image_count: row.14,
        })
        .collect())
}
