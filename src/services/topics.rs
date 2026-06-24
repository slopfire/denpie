use crate::{
    AppState,
    config::topic_icons,
    db::repositories::{token_usage, topics, user_settings},
    domain::topic_visual,
    error::{AppError, AppResult},
    llm,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct TopicService;

impl TopicService {
    pub async fn get_or_create_topic(
        state: &AppState,
        user_id: &str,
        topic_name: &str,
        requested_type: &str,
    ) -> AppResult<topics::TopicRecord> {
        if let Some(mut topic) = topics::find_by_name(&state.db, user_id, topic_name).await? {
            topic = ensure_topic_icon(state, user_id, topic).await?;
            return Ok(topic);
        }

        let icon_id = pick_topic_icon_for_user(state, user_id, topic_name).await?;
        topics::get_or_create_topic(
            &state.db,
            user_id,
            topic_name,
            requested_type,
            Some(&icon_id),
        )
        .await
    }

    pub async fn update_topic_settings(
        state: &AppState,
        user_id: &str,
        topic_id: i64,
        settings: UpdateTopicSettings,
    ) -> AppResult<()> {
        let current = topics::get_settings(&state.db, user_id, topic_id).await?;

        let prompt_template = settings
            .prompt_template
            .map(trimmed_optional)
            .unwrap_or(current.prompt_template);
        let daily_card_count = settings
            .daily_card_count
            .map(|value| {
                if value == 0 {
                    None
                } else {
                    Some(i64::from(value))
                }
            })
            .unwrap_or(current.daily_card_count);
        let daily_time_zone = settings
            .daily_time_zone
            .map(trimmed_optional)
            .unwrap_or(current.daily_time_zone);
        let daily_update_time = settings
            .daily_update_time
            .map(trimmed_optional)
            .unwrap_or(current.daily_update_time);
        let compression_level = settings
            .compression_level
            .map(|value| {
                trimmed_optional(value).map(|value| {
                    llm::CompressionLevel::from_setting(&value)
                        .as_setting()
                        .to_string()
                })
            })
            .unwrap_or(current.compression_level);

        topics::update_settings(
            &state.db,
            user_id,
            topic_id,
            topics::TopicSettingsRecord {
                prompt_template,
                daily_card_count,
                daily_time_zone,
                daily_update_time,
                compression_level,
            },
        )
        .await
    }

    pub async fn delete_topic_by_id(state: &AppState, user_id: &str, id: i64) -> AppResult<()> {
        topics::delete_cascade(&state.db, user_id, id).await
    }

    pub async fn regenerate_topic_icon(
        state: &AppState,
        user_id: &str,
        topic_id: i64,
    ) -> AppResult<TopicVisualUpdate> {
        let topic = topics::find_by_id(&state.db, user_id, topic_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Topic not found".to_string()))?;

        let icon_id = pick_topic_icon_for_user(state, user_id, &topic.name).await?;
        let color_hue = topic_visual::random_color_hue_excluding(topic.color_hue);
        topics::set_topic_visual(&state.db, user_id, topic_id, &icon_id, color_hue).await?;
        Ok(TopicVisualUpdate {
            icon_id,
            topic_color: topic_visual::color_from_hue(color_hue),
        })
    }

    pub async fn list_admin_topics(
        state: &AppState,
        user_id: &str,
    ) -> AppResult<Vec<AdminTopicInfo>> {
        let rows = topics::list_admin(&state.db, user_id).await?;
        Ok(rows
            .into_iter()
            .map(|row| AdminTopicInfo {
                id: row.id,
                name: row.name.clone(),
                tipcard_type: row.tipcard_type,
                icon_id: row
                    .icon_id
                    .unwrap_or_else(|| topic_visual::DEFAULT_TOPIC_ICON.to_string()),
                topic_color: topic_visual::resolve_topic_color(row.color_hue, &row.name),
                prompt_template: row.prompt_template.unwrap_or_default(),
                daily_card_count: row.daily_card_count.unwrap_or(1).max(1) as u32,
                daily_time_zone: row.daily_time_zone.unwrap_or_default(),
                daily_update_time: row.daily_update_time.unwrap_or_default(),
                compression_level: row.compression_level.unwrap_or_default(),
            })
            .collect())
    }

    pub async fn app_summary(
        state: &AppState,
        user_id: &str,
    ) -> AppResult<topics::AppSummaryRecord> {
        topics::app_summary(&state.db, user_id, chrono::Utc::now()).await
    }

    pub async fn list_app_topics(
        state: &AppState,
        user_id: &str,
    ) -> AppResult<Vec<topics::AppTopicRecord>> {
        topics::list_app_topics(&state.db, user_id, chrono::Utc::now()).await
    }
}

pub struct TopicVisualUpdate {
    pub icon_id: String,
    pub topic_color: String,
}

pub struct UpdateTopicSettings {
    pub prompt_template: Option<String>,
    pub daily_card_count: Option<u32>,
    pub daily_time_zone: Option<String>,
    pub daily_update_time: Option<String>,
    pub compression_level: Option<String>,
}

pub struct AdminTopicInfo {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
    pub icon_id: String,
    pub topic_color: String,
    pub prompt_template: String,
    pub daily_card_count: u32,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub compression_level: String,
}

async fn ensure_topic_icon(
    state: &AppState,
    user_id: &str,
    mut topic: topics::TopicRecord,
) -> AppResult<topics::TopicRecord> {
    if topic
        .icon_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|icon| !icon.is_empty())
    {
        return Ok(topic);
    }

    let icon_id = pick_topic_icon_for_user(state, user_id, &topic.name).await?;
    topics::set_icon_id(&state.db, user_id, topic.id, &icon_id).await?;
    topic.icon_id = Some(icon_id);
    Ok(topic)
}

async fn pick_topic_icon_for_user(
    state: &AppState,
    user_id: &str,
    topic_name: &str,
) -> AppResult<String> {
    let defaults = state.settings.get_settings()?;
    let settings = user_settings::get(&state.db, user_id, defaults).await?;
    let allowlist = topic_icons::allowlist();
    let reasoning = llm::ReasoningConfig::new(settings.llm_reasoning_effort.clone());
    let response = llm::pick_topic_icon(
        topic_name,
        allowlist,
        &settings.llm_model,
        &settings.llm_api_key,
        &settings.llm_base_url,
        &reasoning,
    )
    .await;
    token_usage::insert(
        &state.db,
        user_id,
        &settings.llm_model,
        "topic_icon",
        &response.usage,
    )
    .await?;
    Ok(response.content)
}

fn trimmed_optional(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}
