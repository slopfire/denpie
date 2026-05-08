use sqlx::SqlitePool;

use crate::{config::Settings, error::AppResult};

pub async fn get(pool: &SqlitePool, user_id: &str, defaults: Settings) -> AppResult<Settings> {
    let row = sqlx::query_as::<
        _,
        (
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
            String,
            String,
            String,
            String,
            i64,
        ),
    >(
        "SELECT llm_model, llm_compress_model, prompt_template, llm_api_key, llm_base_url,
                llm_compress_base_url, llm_reasoning_effort, llm_compress_reasoning_effort,
                llm_compression_level, color_scheme, transparency, blur_intensity,
                daily_time_zone, daily_update_time, max_active_cards
         FROM user_settings
         WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some(row) => Settings {
            llm_model: row.0,
            llm_compress_model: row.1,
            prompt_template: row.2,
            llm_api_key: row.3,
            llm_base_url: row.4,
            llm_compress_base_url: row.5,
            llm_reasoning_effort: row.6,
            llm_compress_reasoning_effort: row.7,
            llm_compression_level: row.8,
            color_scheme: row.9,
            transparency: row.10,
            blur_intensity: row.11,
            daily_time_zone: row.12,
            daily_update_time: row.13,
            max_active_cards: row.14.max(0) as u64,
            ..defaults
        },
        None => defaults,
    })
}

pub async fn upsert(pool: &SqlitePool, user_id: &str, settings: &Settings) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO user_settings (
            user_id, llm_model, llm_compress_model, prompt_template, llm_api_key,
            llm_base_url, llm_compress_base_url, llm_reasoning_effort,
            llm_compress_reasoning_effort, llm_compression_level, color_scheme,
            transparency, blur_intensity, daily_time_zone, daily_update_time, max_active_cards
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(user_id) DO UPDATE SET
            llm_model = excluded.llm_model,
            llm_compress_model = excluded.llm_compress_model,
            prompt_template = excluded.prompt_template,
            llm_api_key = excluded.llm_api_key,
            llm_base_url = excluded.llm_base_url,
            llm_compress_base_url = excluded.llm_compress_base_url,
            llm_reasoning_effort = excluded.llm_reasoning_effort,
            llm_compress_reasoning_effort = excluded.llm_compress_reasoning_effort,
            llm_compression_level = excluded.llm_compression_level,
            color_scheme = excluded.color_scheme,
            transparency = excluded.transparency,
            blur_intensity = excluded.blur_intensity,
            daily_time_zone = excluded.daily_time_zone,
            daily_update_time = excluded.daily_update_time,
            max_active_cards = excluded.max_active_cards",
    )
    .bind(user_id)
    .bind(&settings.llm_model)
    .bind(&settings.llm_compress_model)
    .bind(&settings.prompt_template)
    .bind(&settings.llm_api_key)
    .bind(&settings.llm_base_url)
    .bind(&settings.llm_compress_base_url)
    .bind(&settings.llm_reasoning_effort)
    .bind(&settings.llm_compress_reasoning_effort)
    .bind(&settings.llm_compression_level)
    .bind(&settings.color_scheme)
    .bind(&settings.transparency)
    .bind(&settings.blur_intensity)
    .bind(&settings.daily_time_zone)
    .bind(&settings.daily_update_time)
    .bind(settings.max_active_cards as i64)
    .execute(pool)
    .await?;
    Ok(())
}
