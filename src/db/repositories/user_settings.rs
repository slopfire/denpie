use sqlx::SqlitePool;

use crate::{config::Settings, error::AppResult};

#[derive(sqlx::FromRow)]
struct SettingsRow {
    llm_model: String,
    llm_grounding_model: String,
    llm_vision_model: String,
    llm_compress_model: String,
    prompt_template: String,
    llm_api_key: String,
    llm_base_url: String,
    llm_compress_base_url: String,
    llm_reasoning_effort: String,
    llm_grounding_reasoning_effort: String,
    llm_compress_reasoning_effort: String,
    llm_compression_level: String,
    daily_time_zone: String,
    daily_update_time: String,
    max_active_cards: i64,
    grounding_strategy: String,
    image_strategy: String,
    search_provider: String,
    scrape_provider: String,
    search_api_key: String,
    search_base_url: String,
    image_sources: String,
}

pub async fn get(pool: &SqlitePool, user_id: &str, defaults: Settings) -> AppResult<Settings> {
    let row = sqlx::query_as::<_, SettingsRow>(
        "SELECT llm_model, llm_grounding_model, llm_vision_model, llm_compress_model, prompt_template,
                llm_api_key, llm_base_url, llm_compress_base_url, llm_reasoning_effort,
                llm_grounding_reasoning_effort, llm_compress_reasoning_effort,
                llm_compression_level, daily_time_zone, daily_update_time, max_active_cards,
                grounding_strategy, image_strategy, search_provider, scrape_provider,
                search_api_key, search_base_url, image_sources
         FROM user_settings
         WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some(row) => Settings {
            llm_model: row.llm_model,
            llm_grounding_model: row.llm_grounding_model,
            llm_vision_model: row.llm_vision_model,
            llm_compress_model: row.llm_compress_model,
            prompt_template: row.prompt_template,
            llm_api_key: row.llm_api_key,
            llm_base_url: row.llm_base_url,
            llm_compress_base_url: row.llm_compress_base_url,
            llm_reasoning_effort: row.llm_reasoning_effort,
            llm_grounding_reasoning_effort: row.llm_grounding_reasoning_effort,
            llm_compress_reasoning_effort: row.llm_compress_reasoning_effort,
            llm_compression_level: row.llm_compression_level,
            daily_time_zone: row.daily_time_zone,
            daily_update_time: row.daily_update_time,
            max_active_cards: row.max_active_cards.max(0) as u64,
            grounding_strategy: row.grounding_strategy,
            image_strategy: row.image_strategy,
            search_provider: row.search_provider,
            scrape_provider: row.scrape_provider,
            search_api_key: row.search_api_key,
            search_base_url: row.search_base_url,
            image_sources: row.image_sources,
            ..defaults
        },
        None => defaults,
    })
}

pub async fn upsert(pool: &SqlitePool, user_id: &str, settings: &Settings) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO user_settings (
            user_id, llm_model, llm_grounding_model, llm_vision_model, llm_compress_model,
            prompt_template, llm_api_key, llm_base_url, llm_compress_base_url,
            llm_reasoning_effort, llm_grounding_reasoning_effort,
            llm_compress_reasoning_effort, llm_compression_level, daily_time_zone,
            daily_update_time, max_active_cards, grounding_strategy, image_strategy,
            search_provider, scrape_provider, search_api_key, search_base_url, image_sources
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(user_id) DO UPDATE SET
            llm_model = excluded.llm_model,
            llm_grounding_model = excluded.llm_grounding_model,
            llm_vision_model = excluded.llm_vision_model,
            llm_compress_model = excluded.llm_compress_model,
            prompt_template = excluded.prompt_template,
            llm_api_key = excluded.llm_api_key,
            llm_base_url = excluded.llm_base_url,
            llm_compress_base_url = excluded.llm_compress_base_url,
            llm_reasoning_effort = excluded.llm_reasoning_effort,
            llm_grounding_reasoning_effort = excluded.llm_grounding_reasoning_effort,
            llm_compress_reasoning_effort = excluded.llm_compress_reasoning_effort,
            llm_compression_level = excluded.llm_compression_level,
            daily_time_zone = excluded.daily_time_zone,
            daily_update_time = excluded.daily_update_time,
            max_active_cards = excluded.max_active_cards,
            grounding_strategy = excluded.grounding_strategy,
            image_strategy = excluded.image_strategy,
            search_provider = excluded.search_provider,
            scrape_provider = excluded.scrape_provider,
            search_api_key = excluded.search_api_key,
            search_base_url = excluded.search_base_url,
            image_sources = excluded.image_sources",
    )
    .bind(user_id)
    .bind(&settings.llm_model)
    .bind(&settings.llm_grounding_model)
    .bind(&settings.llm_vision_model)
    .bind(&settings.llm_compress_model)
    .bind(&settings.prompt_template)
    .bind(&settings.llm_api_key)
    .bind(&settings.llm_base_url)
    .bind(&settings.llm_compress_base_url)
    .bind(&settings.llm_reasoning_effort)
    .bind(&settings.llm_grounding_reasoning_effort)
    .bind(&settings.llm_compress_reasoning_effort)
    .bind(&settings.llm_compression_level)
    .bind(&settings.daily_time_zone)
    .bind(&settings.daily_update_time)
    .bind(settings.max_active_cards as i64)
    .bind(&settings.grounding_strategy)
    .bind(&settings.image_strategy)
    .bind(&settings.search_provider)
    .bind(&settings.scrape_provider)
    .bind(&settings.search_api_key)
    .bind(&settings.search_base_url)
    .bind(&settings.image_sources)
    .execute(pool)
    .await?;
    Ok(())
}
