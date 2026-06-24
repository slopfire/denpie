use crate::{
    AppState,
    api::pb,
    services::topics::{TopicService, UpdateTopicSettings},
};

use super::types::ApiResult;

pub use crate::services::topics::TopicVisualUpdate;

pub(crate) async fn update_topic_prompt(
    state: &AppState,
    user_id: &str,
    req: pb::UpdateTopicRequest,
) -> ApiResult<()> {
    TopicService::update_topic_settings(
        state,
        user_id,
        req.id,
        UpdateTopicSettings {
            prompt_template: req.prompt_template,
            daily_card_count: req.daily_card_count,
            daily_time_zone: req.daily_time_zone,
            daily_update_time: req.daily_update_time,
            compression_level: req.compression_level,
        },
    )
    .await
    .map_err(|err| err.into_status_body())
}

pub async fn delete_topic_by_id(state: &AppState, user_id: &str, id: i64) -> ApiResult<()> {
    TopicService::delete_topic_by_id(state, user_id, id)
        .await
        .map_err(|err| err.into_status_body())
}

#[allow(dead_code)]
pub async fn regenerate_topic_icon(
    state: &AppState,
    user_id: &str,
    topic_id: i64,
) -> ApiResult<TopicVisualUpdate> {
    TopicService::regenerate_topic_icon(state, user_id, topic_id)
        .await
        .map_err(|err| err.into_status_body())
}
