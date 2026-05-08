use crate::{db::repositories::tipcards, domain, AppState};

use super::types::ApiResult;

pub(crate) async fn delete_tipcard_by_id(
    state: &AppState,
    user_id: &str,
    id: i64,
) -> ApiResult<()> {
    tipcards::delete_with_review(&state.db, user_id, id)
        .await
        .map_err(|err| err.into_status_body())
}

pub async fn set_tipcard_pinned(
    state: &AppState,
    user_id: &str,
    id: i64,
    pinned: bool,
) -> ApiResult<()> {
    tipcards::set_pinned(&state.db, user_id, id, pinned)
        .await
        .map_err(|err| err.into_status_body())
}

pub async fn set_tipcard_images(
    state: &AppState,
    user_id: &str,
    id: i64,
    image_data: Vec<String>,
) -> ApiResult<()> {
    let image_data = validate_image_data(image_data)?;
    let image_data_json = image_data_json(&image_data)?;
    tipcards::set_images(&state.db, user_id, id, image_data_json)
        .await
        .map_err(|err| err.into_status_body())
}

pub(crate) fn parse_image_data(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

pub(crate) fn image_data_json(images: &[String]) -> ApiResult<String> {
    serde_json::to_string(images)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub(crate) fn validate_image_data(images: Vec<String>) -> ApiResult<Vec<String>> {
    domain::tipcard::validate_image_data(images)
}

pub(crate) async fn active_card_room(
    state: &AppState,
    user_id: &str,
    max_active_cards: u64,
) -> ApiResult<Option<usize>> {
    if max_active_cards == 0 {
        return Ok(None);
    }
    let active = tipcards::active_card_count(&state.db, user_id)
        .await
        .map_err(|err| err.into_status_body())?
        .max(0) as u64;
    Ok(Some(max_active_cards.saturating_sub(active) as usize))
}
