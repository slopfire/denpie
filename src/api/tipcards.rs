use crate::{AppState, services::tipcards::TipcardService, types::ApiResult};

pub(crate) async fn delete_tipcard_by_id(
    state: &AppState,
    user_id: &str,
    id: i64,
) -> ApiResult<()> {
    TipcardService::delete(state, user_id, id)
        .await
        .map_err(|err| err.into_status_body())
}

pub async fn set_tipcard_pinned(
    state: &AppState,
    user_id: &str,
    id: i64,
    pinned: bool,
) -> ApiResult<()> {
    TipcardService::set_pinned(state, user_id, id, pinned)
        .await
        .map_err(|err| err.into_status_body())
}

pub async fn set_tipcard_images(
    state: &AppState,
    user_id: &str,
    id: i64,
    image_data: Vec<String>,
) -> ApiResult<()> {
    TipcardService::set_images(state, user_id, id, image_data).await
}

pub async fn append_tipcard_images(
    state: &AppState,
    user_id: &str,
    card_id: i64,
    image_data: Vec<String>,
    pool_image_ids: Vec<i64>,
    urls: Vec<String>,
) -> ApiResult<()> {
    TipcardService::append_images(state, user_id, card_id, image_data, pool_image_ids, urls).await
}
