use crate::{
    AppState,
    api::pb,
    services::tips::{CustomTipcardData, TipService},
    types::{
        ApiResult, ForceDailyRefreshRequest, ForceDailyRefreshResponse, TipCardJson,
        TipsJsonRequest,
    },
};

pub async fn build_tips(
    state: &AppState,
    user_id: &str,
    query: TipsJsonRequest,
) -> ApiResult<Vec<TipCardJson>> {
    TipService::build_tips(state, user_id, query).await
}

pub async fn force_daily_refresh(
    state: &AppState,
    user_id: &str,
    req: ForceDailyRefreshRequest,
) -> ApiResult<ForceDailyRefreshResponse> {
    TipService::force_daily_refresh(state, user_id, req).await
}

pub async fn refresh_due_daily_topics(state: &AppState) -> ApiResult<u64> {
    TipService::refresh_due_daily_topics(state).await
}

pub(crate) async fn create_custom_tipcard(
    state: &AppState,
    user_id: &str,
    req: pb::CustomTipcardRequest,
) -> ApiResult<TipCardJson> {
    TipService::create_custom_tipcard(
        state,
        user_id,
        CustomTipcardData {
            topic: req.topic,
            full_content: req.full_content,
            compressed_content: req.compressed_content,
            title: req.title,
        },
    )
    .await
}
