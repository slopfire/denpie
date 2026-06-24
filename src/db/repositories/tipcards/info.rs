use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::error::{AppError, AppResult};

use super::{
    models::{TipcardFilter, TipcardInfoRecord, TipcardRow},
    queries, topic_color_from_row,
};

pub async fn list_admin(pool: &SqlitePool, user_id: &str) -> AppResult<Vec<TipcardInfoRecord>> {
    let sql = format!(
        "{}\n{} WHERE t.user_id = ? ORDER BY pinned DESC, t.created_at DESC",
        queries::BASE_CARD_SELECT,
        queries::CARD_FROM_JOINS
    );
    let rows = sqlx::query_as::<_, TipcardRow>(&sql)
        .bind(user_id)
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(map_info_row).collect())
}

pub async fn list_filtered(
    pool: &SqlitePool,
    user_id: &str,
    filter: TipcardFilter,
) -> AppResult<Vec<TipcardInfoRecord>> {
    let base = format!(
        "{}\n{}",
        queries::BASE_CARD_SELECT,
        queries::CARD_FROM_JOINS
    );
    let mut builder = QueryBuilder::<Sqlite>::new(&base);

    let mut has_where = false;
    push_where(&mut builder, &mut has_where);
    builder.push("t.user_id = ").push_bind(user_id);

    if let Some(q) = filter.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let pattern = format!("%{}%", escape_like(q));
        push_where(&mut builder, &mut has_where);
        builder
            .push("(LOWER(top.name) LIKE LOWER(")
            .push_bind(pattern.clone())
            .push(") ESCAPE '\\' OR LOWER(COALESCE(t.title, '')) LIKE LOWER(")
            .push_bind(pattern.clone())
            .push(") ESCAPE '\\' OR LOWER(t.full_content) LIKE LOWER(")
            .push_bind(pattern.clone())
            .push(") ESCAPE '\\' OR LOWER(t.compressed_content) LIKE LOWER(")
            .push_bind(pattern)
            .push(") ESCAPE '\\')");
    }

    if let Some(status) = filter
        .status
        .as_deref()
        .map(str::trim)
        .filter(|status| !status.is_empty() && *status != "all")
    {
        push_where(&mut builder, &mut has_where);
        builder
            .push("COALESCE(r.status, CASE WHEN top.tipcard_type = 'custom_tip' THEN 'custom' ELSE 'active' END) = ")
            .push_bind(status);
    }

    if let Some(topic) = filter
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
    {
        push_where(&mut builder, &mut has_where);
        builder.push("top.name = ").push_bind(topic);
    }

    if let Some(tipcard_type) = filter
        .tipcard_type
        .as_deref()
        .map(str::trim)
        .filter(|tipcard_type| !tipcard_type.is_empty() && *tipcard_type != "all")
    {
        push_where(&mut builder, &mut has_where);
        builder.push("top.tipcard_type = ").push_bind(tipcard_type);
    }

    builder.push(" ORDER BY pinned DESC, t.created_at DESC LIMIT 500");

    let rows = builder
        .build_query_as::<TipcardRow>()
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(map_info_row).collect())
}

pub async fn get_tipcard_info(
    pool: &SqlitePool,
    user_id: &str,
    id: i64,
) -> AppResult<TipcardInfoRecord> {
    let sql = format!(
        "{}\n{} WHERE t.user_id = ? AND t.id = ?",
        queries::BASE_CARD_SELECT,
        queries::CARD_FROM_JOINS
    );
    let row = sqlx::query_as::<_, TipcardRow>(&sql)
        .bind(user_id)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Tipcard not found".to_string()))?;

    Ok(map_info_row(row))
}

fn map_info_row(row: TipcardRow) -> TipcardInfoRecord {
    TipcardInfoRecord {
        id: row.id,
        topic_name: row.topic_name.clone(),
        topic_icon: row.topic_icon,
        topic_color: topic_color_from_row(&row.topic_name, row.color_hue),
        title: row.title,
        full_content: row.full_content,
        compressed_content: row.compressed_content,
        created_at: row.created_at,
        tipcard_type: row.tipcard_type,
        status: row.status,
        next_review_at: row.next_review_at,
        state_data: row.state_data,
        pinned: row.pinned != 0,
        repeats: row.repeats as u32,
    }
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn push_where(builder: &mut QueryBuilder<Sqlite>, has_where: &mut bool) {
    if *has_where {
        builder.push(" AND ");
    } else {
        builder.push(" WHERE ");
        *has_where = true;
    }
}
