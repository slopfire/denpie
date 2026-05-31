use crate::{
    AppState,
    api::{ApiResult, TopicVisualUpdate},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct TopicService;

impl TopicService {
    pub async fn regenerate_topic_icon(
        state: &AppState,
        user_id: &str,
        topic_id: i64,
    ) -> ApiResult<TopicVisualUpdate> {
        crate::api::regenerate_topic_icon(state, user_id, topic_id).await
    }
}
