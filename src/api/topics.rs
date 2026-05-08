use crate::{db::repositories::topics, llm, AppState};

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
    topics::get_or_create_topic(&state.db, user_id, topic_name, requested_type)
        .await
        .map(Into::into)
        .map_err(|err| err.into_status_body())
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

fn trimmed_optional(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
