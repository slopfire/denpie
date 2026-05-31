use crate::{
    AppState,
    api::{
        ApiResult, ForceDailyRefreshRequest, ForceDailyRefreshResponse, TipCardJson,
        TipsJsonRequest,
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub struct TipService;

impl TipService {
    pub async fn build_tips(
        state: &AppState,
        user_id: &str,
        query: TipsJsonRequest,
    ) -> ApiResult<Vec<TipCardJson>> {
        crate::api::build_tips(state, user_id, query).await
    }

    pub async fn force_daily_refresh(
        state: &AppState,
        user_id: &str,
        req: ForceDailyRefreshRequest,
    ) -> ApiResult<ForceDailyRefreshResponse> {
        crate::api::force_daily_refresh(state, user_id, req).await
    }
}
