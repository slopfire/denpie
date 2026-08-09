use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::{
    domain::{tipcard, topic_visual},
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
    pub icon_id: Option<String>,
    pub color_hue: Option<i32>,
    pub grounding_strategy: Option<String>,
    pub image_strategy: Option<String>,
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
    pub grounding_strategy: String,
    pub image_strategy: String,
    pub icon_id: String,
    pub topic_color: String,
    pub total_cards: i64,
    pub due_cards: i64,
    pub pending_cards: i64,
    pub completed_cards: i64,
}

#[derive(Clone, Debug)]
pub struct TopicSettingsRecord {
    pub prompt_template: Option<String>,
    pub daily_card_count: Option<i64>,
    pub daily_time_zone: Option<String>,
    pub daily_update_time: Option<String>,
    pub compression_level: Option<String>,
    pub grounding_strategy: Option<String>,
    pub image_strategy: Option<String>,
}

type TopicRow = (
    i64,
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<String>,
    Option<String>,
);

fn map_topic_row(row: TopicRow) -> TopicRecord {
    TopicRecord {
        id: row.0,
        name: row.1,
        tipcard_type: row.2,
        prompt_template: row.3,
        daily_card_count: row.4,
        daily_time_zone: row.5,
        daily_update_time: row.6,
        compression_level: row.7,
        icon_id: row.8,
        color_hue: row.9.map(|hue| hue as i32),
        grounding_strategy: row.10,
        image_strategy: row.11,
    }
}

const TOPIC_SELECT: &str = "SELECT id, name, tipcard_type, prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, icon_id, color_hue, grounding_strategy, image_strategy";

pub async fn list_admin(pool: &PgPool, user_id: &str) -> AppResult<Vec<TopicRecord>> {
    let query = format!("{TOPIC_SELECT} FROM topics WHERE user_id = $1 ORDER BY name ASC");
    let rows = sqlx::query_as::<_, TopicRow>(&query)
        .bind(user_id)
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(map_topic_row).collect())
}

pub async fn list_names(pool: &PgPool, user_id: &str) -> AppResult<Vec<String>> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT name FROM topics WHERE user_id = $1 ORDER BY name ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

pub async fn list_generated_targets(
    pool: &PgPool,
    user_id: &str,
) -> AppResult<Vec<(TopicRecord, String)>> {
    let query = format!(
        "{TOPIC_SELECT} FROM topics WHERE user_id = $1 AND tipcard_type NOT IN ('manual_tip', 'custom_tip')"
    );
    let rows = sqlx::query_as::<_, TopicRow>(&query)
        .bind(user_id)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let tipcard_type = row.2.clone();
            (map_topic_row(row), tipcard_type)
        })
        .collect())
}

pub async fn find_by_id(pool: &PgPool, user_id: &str, id: i64) -> AppResult<Option<TopicRecord>> {
    let query = format!("{TOPIC_SELECT} FROM topics WHERE user_id = $1 AND id = $2");
    let row = sqlx::query_as::<_, TopicRow>(&query)
        .bind(user_id)
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(map_topic_row))
}

pub async fn find_by_name(
    pool: &PgPool,
    user_id: &str,
    topic_name: &str,
) -> AppResult<Option<TopicRecord>> {
    let query = format!("{TOPIC_SELECT} FROM topics WHERE user_id = $1 AND name = $2");
    let row = sqlx::query_as::<_, TopicRow>(&query)
        .bind(user_id)
        .bind(topic_name)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(map_topic_row))
}

pub async fn get_or_create_topic(
    pool: &PgPool,
    user_id: &str,
    topic_name: &str,
    requested_type: &str,
    icon_id: Option<&str>,
) -> AppResult<TopicRecord> {
    if let Some(topic) = find_by_name(pool, user_id, topic_name).await? {
        return Ok(topic);
    }

    let tipcard_type = tipcard::normalize_tipcard_type(requested_type, topic_name);
    let icon_id = icon_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| topic_visual::DEFAULT_TOPIC_ICON.to_string());
    let color_hue = topic_visual::color_hue_from_name(topic_name);

    match sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type, icon_id, color_hue)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_id, name) DO NOTHING
         RETURNING id",
    )
    .bind(user_id)
    .bind(topic_name)
    .bind(&tipcard_type)
    .bind(&icon_id)
    .bind(color_hue)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(id)) => Ok(TopicRecord {
            id,
            name: topic_name.to_string(),
            tipcard_type,
            prompt_template: None,
            daily_card_count: None,
            daily_time_zone: None,
            daily_update_time: None,
            compression_level: None,
            icon_id: Some(icon_id),
            color_hue: Some(color_hue),
            grounding_strategy: None,
            image_strategy: None,
        }),
        Ok(None) => find_by_name(pool, user_id, topic_name)
            .await?
            .ok_or_else(|| AppError::NotFound("Topic disappeared during creation".to_string())),
        Err(insert_error) => Err(AppError::Db(insert_error)),
    }
}

pub async fn set_topic_visual(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    icon_id: &str,
    color_hue: i32,
) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE topics SET icon_id = $1, color_hue = $2 WHERE id = $3 AND user_id = $4",
    )
    .bind(icon_id)
    .bind(color_hue)
    .bind(topic_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Topic not found".to_string()));
    }
    Ok(())
}

pub async fn set_icon_id(
    pool: &PgPool,
    user_id: &str,
    topic_id: i64,
    icon_id: &str,
) -> AppResult<()> {
    let result = sqlx::query("UPDATE topics SET icon_id = $1 WHERE id = $2 AND user_id = $3")
        .bind(icon_id)
        .bind(topic_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Topic not found".to_string()));
    }
    Ok(())
}

pub async fn delete_cascade(pool: &PgPool, user_id: &str, id: i64) -> AppResult<Vec<String>> {
    let mut tx = pool.begin().await?;
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM topics WHERE id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("Topic not found".to_string()));
    }

    let image_paths = sqlx::query_scalar::<_, String>(
        "DELETE FROM tipcard_images
         WHERE user_id = $1
           AND card_id IN (SELECT id FROM tipcards WHERE topic_id = $2 AND user_id = $3)
         RETURNING storage_path",
    )
    .bind(user_id)
    .bind(id)
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;

    // review states and daily allowances cascade from cards/topics.
    sqlx::query("DELETE FROM tipcards WHERE topic_id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM topics WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(image_paths)
}

pub async fn get_settings(pool: &PgPool, user_id: &str, id: i64) -> AppResult<TopicSettingsRecord> {
    let row = sqlx::query_as::<
        _,
        (
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT prompt_template, daily_card_count, daily_time_zone, daily_update_time, compression_level, grounding_strategy, image_strategy
         FROM topics
         WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Topic not found".to_string()))?;

    Ok(TopicSettingsRecord {
        prompt_template: row.0,
        daily_card_count: row.1,
        daily_time_zone: row.2,
        daily_update_time: row.3,
        compression_level: row.4,
        grounding_strategy: row.5,
        image_strategy: row.6,
    })
}

pub async fn update_settings(
    pool: &PgPool,
    user_id: &str,
    id: i64,
    settings: TopicSettingsRecord,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE topics
         SET prompt_template = $1, daily_card_count = $2, daily_time_zone = $3, daily_update_time = $4, compression_level = $5, grounding_strategy = $6, image_strategy = $7
         WHERE id = $8 AND user_id = $9",
    )
    .bind(settings.prompt_template)
    .bind(settings.daily_card_count)
    .bind(settings.daily_time_zone)
    .bind(settings.daily_update_time)
    .bind(settings.compression_level)
    .bind(settings.grounding_strategy)
    .bind(settings.image_strategy)
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn app_summary(
    pool: &PgPool,
    user_id: &str,
    now: DateTime<Utc>,
) -> AppResult<AppSummaryRecord> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT 
            (SELECT COUNT(*) FROM topics WHERE user_id = $1) as topics,
            (SELECT COUNT(*) FROM tipcards WHERE user_id = $2) as total_cards,
            (SELECT COUNT(*) FROM review_states r JOIN tipcards t ON t.id = r.card_id WHERE t.user_id = $3 AND r.status = 'active' AND (r.next_review_at <= $4 OR t.pinned = 1)) as due_cards,
            (SELECT COUNT(*) FROM review_states r JOIN tipcards t ON t.id = r.card_id WHERE t.user_id = $5 AND r.status = 'active') as active_cards",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(now)
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(AppSummaryRecord {
        topics: row.0,
        total_cards: row.1,
        due_cards: row.2,
        active_cards: row.3,
    })
}

pub async fn list_app_topics(
    pool: &PgPool,
    user_id: &str,
    now: DateTime<Utc>,
) -> AppResult<Vec<AppTopicRecord>> {
    let rows = sqlx::query(
        "SELECT top.id,
                top.name,
                top.tipcard_type,
                top.prompt_template,
                top.daily_card_count,
                top.daily_time_zone,
                top.daily_update_time,
                top.compression_level,
                top.icon_id,
                top.color_hue,
                top.grounding_strategy,
                top.image_strategy,
                COUNT(t.id) AS total_cards,
                SUM(CASE WHEN r.status = 'active' AND (r.next_review_at <= $1 OR t.pinned = 1) THEN 1 ELSE 0 END) AS due_cards,
                SUM(CASE WHEN r.status = 'pending' THEN 1 ELSE 0 END) AS pending_cards,
                SUM(CASE WHEN r.status != 'active' THEN 1 ELSE 0 END) AS completed_cards
         FROM topics top
         LEFT JOIN tipcards t ON t.topic_id = top.id
         LEFT JOIN review_states r ON r.card_id = t.id
         WHERE top.user_id = $2
         GROUP BY top.id, top.name, top.tipcard_type, top.prompt_template, top.daily_card_count, top.daily_time_zone, top.daily_update_time, top.compression_level, top.icon_id, top.color_hue, top.grounding_strategy, top.image_strategy
         ORDER BY due_cards DESC, top.name ASC",
    )
    .bind(now)
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let name: String = row.get("name");
            AppTopicRecord {
                id: row.get("id"),
                name: name.clone(),
                tipcard_type: row.get("tipcard_type"),
                prompt_template: row
                    .get::<Option<String>, _>("prompt_template")
                    .unwrap_or_default(),
                daily_card_count: row
                    .get::<Option<i64>, _>("daily_card_count")
                    .unwrap_or(1)
                    .max(1) as u32,
                daily_time_zone: row
                    .get::<Option<String>, _>("daily_time_zone")
                    .unwrap_or_default(),
                daily_update_time: row
                    .get::<Option<String>, _>("daily_update_time")
                    .unwrap_or_default(),
                compression_level: row
                    .get::<Option<String>, _>("compression_level")
                    .unwrap_or_default(),
                grounding_strategy: row
                    .get::<Option<String>, _>("grounding_strategy")
                    .unwrap_or_default(),
                image_strategy: row
                    .get::<Option<String>, _>("image_strategy")
                    .unwrap_or_default(),
                icon_id: row
                    .get::<Option<String>, _>("icon_id")
                    .unwrap_or_else(|| topic_visual::DEFAULT_TOPIC_ICON.to_string()),
                topic_color: topic_visual::resolve_topic_color(
                    row.get::<Option<i64>, _>("color_hue").map(|hue| hue as i32),
                    &name,
                ),
                total_cards: row.get("total_cards"),
                due_cards: row.get("due_cards"),
                pending_cards: row.get("pending_cards"),
                completed_cards: row.get("completed_cards"),
            }
        })
        .collect())
}
