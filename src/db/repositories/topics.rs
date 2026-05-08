use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::{
    domain::tipcard,
    error::{AppError, AppResult},
};

#[derive(Clone, Debug)]
pub struct TopicRecord {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
    pub prompt_template: Option<String>,
    pub daily_card_count: Option<i64>,
    pub daily_time_zone: Option<String>,
    pub daily_update_time: Option<String>,
    pub compression_level: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AppSummaryRecord {
    pub topics: i64,
    pub total_cards: i64,
    pub due_cards: i64,
    pub active_cards: i64,
}

#[derive(Clone, Debug)]
pub struct AppTopicRecord {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
    pub prompt_template: String,
    pub daily_card_count: u32,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub compression_level: String,
    pub total_cards: i64,
    pub due_cards: i64,
    pub completed_cards: i64,
}

#[derive(Clone, Debug)]
pub struct TopicSettingsRecord {
    pub prompt_template: Option<String>,
    pub daily_card_count: Option<i64>,
    pub daily_time_zone: Option<String>,
    pub daily_update_time: Option<String>,
    pub compression_level: Option<String>,
}

pub async fn list_admin(pool: &SqlitePool) -> AppResult<Vec<TopicRecord>> {
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level
         FROM topics
         ORDER BY name ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| TopicRecord {
            id: row.0,
            name: row.1,
            tipcard_type: row.2,
            prompt_template: row.3,
            daily_card_count: row.4,
            daily_time_zone: row.5,
            daily_update_time: row.6,
            compression_level: row.7,
        })
        .collect())
}

pub async fn list_names(pool: &SqlitePool) -> AppResult<Vec<String>> {
    Ok(
        sqlx::query_scalar::<_, String>("SELECT name FROM topics ORDER BY name ASC")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn list_generated_targets(pool: &SqlitePool) -> AppResult<Vec<(TopicRecord, String)>> {
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level
         FROM topics
         WHERE tipcard_type NOT IN ('manual_tip', 'custom_tip')",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let tipcard_type = row.2.clone();
            (
                TopicRecord {
                    id: row.0,
                    name: row.1,
                    tipcard_type: row.2,
                    prompt_template: row.3,
                    daily_card_count: row.4,
                    daily_time_zone: row.5,
                    daily_update_time: row.6,
                    compression_level: row.7,
                },
                tipcard_type,
            )
        })
        .collect())
}

pub async fn get_or_create_topic(
    pool: &SqlitePool,
    topic_name: &str,
    requested_type: &str,
) -> AppResult<TopicRecord> {
    if let Some(topic) = get_topic(pool, topic_name).await? {
        // If type differs, we could update it, but for now just return
        return Ok(topic);
    }

    let tipcard_type = tipcard::normalize_tipcard_type(requested_type, topic_name);

    match sqlx::query("INSERT INTO topics (name, tipcard_type) VALUES (?, ?)")
        .bind(topic_name)
        .bind(&tipcard_type)
        .execute(pool)
        .await
    {
        Ok(result) => Ok(TopicRecord {
            id: result.last_insert_rowid(),
            name: topic_name.to_string(),
            tipcard_type,
            prompt_template: None,
            daily_card_count: None,
            daily_time_zone: None,
            daily_update_time: None,
            compression_level: None,
        }),
        Err(insert_error) => get_topic(pool, topic_name)
            .await?
            .ok_or_else(|| AppError::Db(insert_error)),
    }
}

pub async fn delete_cascade(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "DELETE FROM review_states
         WHERE card_id IN (SELECT id FROM tipcards WHERE topic_id = ?)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM tipcards WHERE topic_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    let result = sqlx::query("DELETE FROM topics WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Topic not found".to_string()));
    }

    tx.commit().await?;
    Ok(())
}

pub async fn get_settings(pool: &SqlitePool, id: i64) -> AppResult<TopicSettingsRecord> {
    let row = sqlx::query_as::<
        _,
        (
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level
         FROM topics
         WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Topic not found".to_string()))?;

    Ok(TopicSettingsRecord {
        prompt_template: row.0,
        daily_card_count: row.1,
        daily_time_zone: row.2,
        daily_update_time: row.3,
        compression_level: row.4,
    })
}

pub async fn update_settings(
    pool: &SqlitePool,
    id: i64,
    settings: TopicSettingsRecord,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE topics
         SET prompt_template = ?, daily_card_count = ?, daily_time_zone = ?, daily_update_time = ?, compression_level = ?
         WHERE id = ?",
    )
    .bind(settings.prompt_template)
    .bind(settings.daily_card_count)
    .bind(settings.daily_time_zone)
    .bind(settings.daily_update_time)
    .bind(settings.compression_level)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn app_summary(pool: &SqlitePool, now: DateTime<Utc>) -> AppResult<AppSummaryRecord> {
    let topics = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM topics")
        .fetch_one(pool)
        .await?;
    let total_cards = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tipcards")
        .fetch_one(pool)
        .await?;
    let due_cards = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE r.status = 'active' AND (r.next_review_at <= ? OR t.pinned = 1)",
    )
    .bind(now)
    .fetch_one(pool)
    .await?;
    let active_cards =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM review_states WHERE status = 'active'")
            .fetch_one(pool)
            .await?;

    Ok(AppSummaryRecord {
        topics,
        total_cards,
        due_cards,
        active_cards,
    })
}

pub async fn list_app_topics(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> AppResult<Vec<AppTopicRecord>> {
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            i64,
            i64,
        ),
    >(
        "SELECT top.id,
                top.name,
                top.tipcard_type,
                top.prompt_template,
                top.daily_card_count,
                top.daily_time_zone,
                top.daily_update_time,
                top.compression_level,
                COUNT(t.id) AS total_cards,
                SUM(CASE WHEN r.status = 'active' AND (r.next_review_at <= ? OR t.pinned = 1) THEN 1 ELSE 0 END) AS due_cards,
                SUM(CASE WHEN r.status != 'active' THEN 1 ELSE 0 END) AS completed_cards
         FROM topics top
         LEFT JOIN tipcards t ON t.topic_id = top.id
         LEFT JOIN review_states r ON r.card_id = t.id
         GROUP BY top.id, top.name, top.tipcard_type, top.prompt_template, top.daily_card_count, top.daily_time_zone, top.daily_update_time, top.compression_level
         ORDER BY due_cards DESC, top.name ASC",
    )
    .bind(now)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| AppTopicRecord {
            id: row.0,
            name: row.1,
            tipcard_type: row.2,
            prompt_template: row.3.unwrap_or_default(),
            daily_card_count: row.4.unwrap_or(1).max(1) as u32,
            daily_time_zone: row.5.unwrap_or_default(),
            daily_update_time: row.6.unwrap_or_default(),
            compression_level: row.7.unwrap_or_default(),
            total_cards: row.8,
            due_cards: row.9,
            completed_cards: row.10,
        })
        .collect())
}

async fn get_topic(pool: &SqlitePool, topic_name: &str) -> AppResult<Option<TopicRecord>> {
    let row = sqlx::query_as::<
        _,
        (
            i64,
            String,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT id, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level
         FROM topics
         WHERE name = ?",
    )
    .bind(topic_name)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| TopicRecord {
        id: row.0,
        name: topic_name.to_string(),
        tipcard_type: row.1,
        prompt_template: row.2,
        daily_card_count: row.3,
        daily_time_zone: row.4,
        daily_update_time: row.5,
        compression_level: row.6,
    }))
}
