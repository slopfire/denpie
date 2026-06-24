use std::collections::HashMap;

use crate::{
    AppState, db::repositories::tipcards as tipcards_repo, domain, image_store, types::ApiResult,
};

pub use tipcards_repo::TipcardFilter;

#[derive(Clone, Copy, Debug, Default)]
pub struct TipcardService;

impl TipcardService {
    pub async fn list_tipcards(
        state: &AppState,
        user_id: &str,
        filter: tipcards_repo::TipcardFilter,
    ) -> crate::error::AppResult<Vec<tipcards_repo::TipcardInfoRecord>> {
        tipcards_repo::list_filtered(&state.db, user_id, filter).await
    }

    pub async fn list_flow_cards(
        state: &AppState,
        user_id: &str,
        cursor: Option<(i64, String, i64)>,
        limit: i64,
    ) -> crate::error::AppResult<Vec<tipcards_repo::FlowCardRecord>> {
        tipcards_repo::list_flow_cards(&state.db, user_id, cursor, limit).await
    }

    pub async fn tipcard_detail(
        state: &AppState,
        user_id: &str,
        id: i64,
    ) -> crate::error::AppResult<(
        tipcards_repo::TipcardInfoRecord,
        Vec<tipcards_repo::TipcardImageRecord>,
    )> {
        let info = tipcards_repo::get_tipcard_info(&state.db, user_id, id).await?;
        let images = tipcards_repo::list_images(&state.db, user_id, id).await?;
        Ok((info, images))
    }

    pub async fn list_images_for_cards(
        state: &AppState,
        user_id: &str,
        card_ids: &[i64],
    ) -> crate::error::AppResult<HashMap<i64, Vec<tipcards_repo::TipcardImageRecord>>> {
        tipcards_repo::list_images_for_cards(&state.db, user_id, card_ids).await
    }

    pub async fn delete(state: &AppState, user_id: &str, id: i64) -> crate::error::AppResult<()> {
        tipcards_repo::delete_with_review(&state.db, user_id, id).await
    }

    pub async fn set_pinned(
        state: &AppState,
        user_id: &str,
        id: i64,
        pinned: bool,
    ) -> crate::error::AppResult<()> {
        tipcards_repo::set_pinned(&state.db, user_id, id, pinned).await
    }

    pub async fn set_images(
        state: &AppState,
        user_id: &str,
        id: i64,
        image_data: Vec<String>,
    ) -> ApiResult<()> {
        let image_data = validate_image_data(image_data)?;
        image_store::replace_card_images(&state.db, &state.image_dir, user_id, id, image_data).await
    }
}

pub(crate) fn parse_image_data(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

pub(crate) fn image_data_json(images: &[String]) -> ApiResult<String> {
    serde_json::to_string(images)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub(crate) fn validate_image_data(images: Vec<String>) -> ApiResult<Vec<String>> {
    domain::tipcard::validate_image_data(images).map_err(|err| err.into_status_body())
}

pub(crate) async fn active_card_room(
    state: &AppState,
    user_id: &str,
    max_active_cards: u64,
) -> ApiResult<Option<usize>> {
    if max_active_cards == 0 {
        return Ok(None);
    }
    let active = tipcards_repo::active_card_count(&state.db, user_id)
        .await
        .map_err(|err| err.into_status_body())?
        .max(0) as u64;
    Ok(Some(max_active_cards.saturating_sub(active) as usize))
}
