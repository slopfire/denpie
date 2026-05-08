use chrono::{DateTime, Utc};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::{
    domain::review::RepeatableState,
    error::{AppError, AppResult},
};

#[derive(Clone, Debug)]
pub struct ScheduledCardRecord {
    pub id: i64,
    pub full_content: String,
    pub compressed_content: String,
    pub pinned: bool,
    pub image_data: String,
}

#[derive(Clone, Debug)]
pub struct TipcardInfoRecord {
    pub id: i64,
    pub topic_name: String,
    pub title: String,
    pub full_content: String,
    pub compressed_content: String,
    pub image_data: String,
    pub created_at: String,
    pub tipcard_type: String,
    pub status: String,
    pub next_review_at: String,
    pub state_data: String,
    pub pinned: bool,
}

#[derive(Clone, Debug, Default)]
pub struct TipcardFilter {
    pub q: Option<String>,
    pub status: Option<String>,
    pub topic: Option<String>,
    pub tipcard_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CardContextTitleRecord {
    pub title: String,
    pub status: String,
}

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

pub async fn set_images(
    pool: &SqlitePool,
    user_id: &str,
    id: i64,
    image_data_json: String,
) -> AppResult<()> {
    let result = sqlx::query("UPDATE tipcards SET image_data = ? WHERE id = ? AND user_id = ?")
        .bind(image_data_json)
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Card not found".to_string()));
    }

    Ok(())
}

pub async fn list_admin(pool: &SqlitePool, user_id: &str) -> AppResult<Vec<TipcardInfoRecord>> {
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
        ),
    >(
        "SELECT t.id,
                top.name AS topic_name,
                t.full_content,
                t.compressed_content,
                COALESCE(CAST(t.created_at AS TEXT), '') AS created_at,
                top.tipcard_type,
                COALESCE(r.status, CASE WHEN top.tipcard_type = 'custom_tip' THEN 'custom' ELSE 'active' END) AS status,
                COALESCE(CAST(r.next_review_at AS TEXT), '') AS next_review_at,
                COALESCE(r.state_data, '') AS state_data,
                COALESCE(t.pinned, 0) AS pinned
         FROM tipcards t
         JOIN topics top ON t.topic_id = top.id
         LEFT JOIN review_states r ON r.card_id = t.id
         WHERE t.user_id = ?
         ORDER BY pinned DESC, t.created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| TipcardInfoRecord {
            id: row.0,
            topic_name: row.1,
            title: String::new(),
            full_content: row.2,
            compressed_content: row.3,
            image_data: "[]".to_string(),
            created_at: row.4,
            tipcard_type: row.5,
            status: row.6,
            next_review_at: row.7,
            state_data: row.8,
            pinned: row.9 != 0,
        })
        .collect())
}

pub async fn list_filtered(
    pool: &SqlitePool,
    user_id: &str,
    filter: TipcardFilter,
) -> AppResult<Vec<TipcardInfoRecord>> {
    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT t.id,
                top.name AS topic_name,
                COALESCE(t.title, '') AS title,
                t.full_content,
                t.compressed_content,
                COALESCE(t.image_data, '[]') AS image_data,
                COALESCE(CAST(t.created_at AS TEXT), '') AS created_at,
                top.tipcard_type,
                COALESCE(r.status, CASE WHEN top.tipcard_type = 'custom_tip' THEN 'custom' ELSE 'active' END) AS status,
                COALESCE(CAST(r.next_review_at AS TEXT), '') AS next_review_at,
                COALESCE(r.state_data, '') AS state_data,
                COALESCE(t.pinned, 0) AS pinned
         FROM tipcards t
         JOIN topics top ON t.topic_id = top.id
         LEFT JOIN review_states r ON r.card_id = t.id",
    );

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
        .build_query_as::<(
            i64,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
        )>()
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| TipcardInfoRecord {
            id: row.0,
            topic_name: row.1,
            title: row.2,
            full_content: row.3,
            compressed_content: row.4,
            image_data: row.5,
            created_at: row.6,
            tipcard_type: row.7,
            status: row.8,
            next_review_at: row.9,
            state_data: row.10,
            pinned: row.11 != 0,
        })
        .collect())
}

pub async fn find_daily_topic_cards(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    daily_window_start: DateTime<Utc>,
    exclude_card_ids: &[i64],
    limit: usize,
) -> AppResult<Vec<ScheduledCardRecord>> {
    let mut daily_query = QueryBuilder::<Sqlite>::new(
        "
        SELECT t.id, t.full_content, t.compressed_content, COALESCE(t.pinned, 0), COALESCE(t.image_data, '[]')
        FROM tipcards t
        JOIN review_states r ON t.id = r.card_id
        WHERE t.user_id = ",
    );
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
    let mut due_query = QueryBuilder::<Sqlite>::new(
        "
        SELECT t.id, t.full_content, t.compressed_content, COALESCE(t.pinned, 0), COALESCE(t.image_data, '[]')
        FROM tipcards t
        JOIN review_states r ON t.id = r.card_id
        WHERE t.user_id = ",
    );
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
        "
        ORDER BY
            t.pinned DESC,
            CASE
                WHEN t.tipcard_type = 'repeatable_tip'
                     AND COALESCE(CAST(json_extract(r.state_data, '$.repeats') AS INTEGER), 0) > 0
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
    let algo = state.scheduling_state.algorithm.storage_name();
    let state_json = serde_json::to_string(&state)?;

    create_review_state(pool, card_id, algo, state_json, Utc::now()).await?;
    Ok(card_id)
}

pub async fn create_manual(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    title: &str,
    full_content: &str,
    compressed_content: &str,
    image_data_json: &str,
) -> AppResult<i64> {
    let card_id = sqlx::query(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content, image_data) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(topic_id)
    .bind(tipcard_type)
    .bind(title)
    .bind(full_content)
    .bind(compressed_content)
    .bind(image_data_json)
    .execute(pool)
    .await?
    .last_insert_rowid();

    create_review_state(
        pool,
        card_id,
        RepeatableState::default()
            .scheduling_state
            .algorithm
            .storage_name(),
        serde_json::to_string(&RepeatableState::default())?,
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

pub async fn list_context_titles(
    pool: &SqlitePool,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
    limit: i64,
) -> AppResult<Vec<CardContextTitleRecord>> {
    let rows = sqlx::query_as::<_, (String, String)>(
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

    Ok(rows
        .into_iter()
        .map(|row| CardContextTitleRecord {
            title: row.0,
            status: row.1,
        })
        .collect())
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

async fn create_review_state(
    pool: &SqlitePool,
    card_id: i64,
    algorithm_used: &str,
    state_data: String,
    next_review_at: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES (?, ?, ?, 'active', ?)",
    )
    .bind(card_id)
    .bind(algorithm_used)
    .bind(state_data)
    .bind(next_review_at)
    .execute(pool)
    .await?;
    Ok(())
}
