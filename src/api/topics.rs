use axum::http::StatusCode;

use crate::{
    config::topic_icons,
    db::repositories::{token_usage, topics, user_settings},
    domain::topic_visual,
    llm, AppState,
};

use super::{
    pb,
    types::{ApiResult, TopicInfo},
};

pub(crate) async fn get_or_create_topic(
    state: &AppState,
    user_id: &str,
    topic_name: &str,
    requested_type: &str,
) -> ApiResult<TopicInfo> {
    if let Some(mut topic) = topics::find_by_name(&state.db, user_id, topic_name)
        .await
        .map_err(|err| err.into_status_body())?
    {
        topic = ensure_topic_icon(state, user_id, topic).await?;
        return Ok(topic.into());
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
    .map(Into::into)
    .map_err(|err| err.into_status_body())
}

async fn ensure_topic_icon(
    state: &AppState,
    user_id: &str,
    mut topic: topics::TopicRecord,
) -> ApiResult<topics::TopicRecord> {
    if topic
        .icon_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|icon| !icon.is_empty())
    {
        return Ok(topic);
    }

    let icon_id = pick_topic_icon_for_user(state, user_id, &topic.name).await?;
    topics::set_icon_id(&state.db, user_id, topic.id, &icon_id)
        .await
        .map_err(|err| err.into_status_body())?;
    topic.icon_id = Some(icon_id);
    Ok(topic)
}

async fn pick_topic_icon_for_user(
    state: &AppState,
    user_id: &str,
    topic_name: &str,
) -> ApiResult<String> {
    let defaults = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    let settings = user_settings::get(&state.db, user_id, defaults)
        .await
        .map_err(|err| err.into_status_body())?;
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
    .await
    .map_err(|err| err.into_status_body())?;
    Ok(response.content)
}

pub(crate) async fn update_topic_prompt(
    state: &AppState,
    user_id: &str,
    req: pb::UpdateTopicRequest,
) -> ApiResult<()> {
    let current = topics::get_settings(&state.db, user_id, req.id)
        .await
        .map_err(|err| err.into_status_body())?;

    let prompt_template = req
        .prompt_template
        .map(trimmed_optional)
        .unwrap_or(current.prompt_template);
    let daily_card_count = req
        .daily_card_count
        .map(|value| {
            if value == 0 {
                None
            } else {
                Some(i64::from(value))
            }
        })
        .unwrap_or(current.daily_card_count);
    let daily_time_zone = req
        .daily_time_zone
        .map(trimmed_optional)
        .unwrap_or(current.daily_time_zone);
    let daily_update_time = req
        .daily_update_time
        .map(trimmed_optional)
        .unwrap_or(current.daily_update_time);
    let compression_level = req
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
        req.id,
        topics::TopicSettingsRecord {
            prompt_template,
            daily_card_count,
            daily_time_zone,
            daily_update_time,
            compression_level,
        },
    )
    .await
    .map_err(|err| err.into_status_body())
}

pub async fn delete_topic_by_id(state: &AppState, user_id: &str, id: i64) -> ApiResult<()> {
    topics::delete_cascade(&state.db, user_id, id)
        .await
        .map_err(|err| err.into_status_body())
}

pub struct TopicVisualUpdate {
    pub icon_id: String,
    pub topic_color: String,
}

pub async fn regenerate_topic_icon(
    state: &AppState,
    user_id: &str,
    topic_id: i64,
) -> ApiResult<TopicVisualUpdate> {
    let topic = topics::find_by_id(&state.db, user_id, topic_id)
        .await
        .map_err(|err| err.into_status_body())?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Topic not found".to_string()))?;

    let icon_id = pick_topic_icon_for_user(state, user_id, &topic.name).await?;
    let color_hue = topic_visual::random_color_hue_excluding(topic.color_hue);
    topics::set_topic_visual(&state.db, user_id, topic_id, &icon_id, color_hue)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(TopicVisualUpdate {
        icon_id,
        topic_color: topic_visual::color_from_hue(color_hue),
    })
}

fn trimmed_optional(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
