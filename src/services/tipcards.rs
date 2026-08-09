use std::collections::HashMap;

use crate::{
    AppState,
    db::repositories::{image_pool, tipcards as tipcards_repo, topics, user_settings},
    domain, image_store,
    types::ApiResult,
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
        // Sweep legacy generation-failure placeholders ("Failed parsing text",
        // "LLM Error: ...") out of queues so they can never surface in the flow.
        let (purged, image_paths) =
            tipcards_repo::delete_failed_generation_cards(&state.db, user_id).await?;
        image_store::remove_stored_files(&state.image_dir, &image_paths).await;
        if purged > 0 {
            tracing::warn!(user_id, purged, "purged failed generation cards");
        }
        tipcards_repo::stack_due_repeatable_cards(&state.db, user_id).await?;
        Self::promote_pending_within_daily_limit(state, user_id).await?;
        tipcards_repo::list_flow_cards(&state.db, user_id, cursor, limit).await
    }

    /// Make one queued repeatable card visible only while its topic has room in
    /// the learner's current daily set. Scheduled cards remain governed by SM-2.
    pub async fn promote_pending_within_daily_limit(
        state: &AppState,
        user_id: &str,
    ) -> crate::error::AppResult<()> {
        let defaults = state.settings.get_settings()?;
        let settings = user_settings::get(&state.db, user_id, defaults).await?;
        let targets = topics::list_generated_targets(&state.db, user_id)
            .await?
            .into_iter()
            .filter(|(_, tipcard_type)| tipcard_type == "repeatable_tip")
            .map(|(topic, _)| tipcards_repo::DailyReviewTarget {
                topic_id: topic.id,
                window_start: domain::scheduling::topic_daily_window_start(
                    &topic,
                    &settings.daily_time_zone,
                    &settings.daily_update_time,
                ),
                daily_card_count: domain::scheduling::topic_daily_card_count(&topic) as i64,
            })
            .collect::<Vec<_>>();
        tipcards_repo::promote_pending_within_daily_limits(&state.db, user_id, &targets).await
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
        let image_paths = tipcards_repo::delete_with_review(&state.db, user_id, id).await?;
        image_store::remove_stored_files(&state.image_dir, &image_paths).await;
        Ok(())
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

    pub async fn append_images(
        state: &AppState,
        user_id: &str,
        card_id: i64,
        image_data: Vec<String>,
        pool_image_ids: Vec<i64>,
        urls: Vec<String>,
    ) -> ApiResult<()> {
        // Verify ownership before any remote request or filesystem write.
        let existing = tipcards_repo::list_images(&state.db, user_id, card_id)
            .await
            .map_err(|err| err.into_status_body())?;
        tipcards_repo::get_tipcard_info(&state.db, user_id, card_id)
            .await
            .map_err(|err| err.into_status_body())?;
        let image_data = validate_image_data(image_data)?;
        let urls: Vec<String> = urls
            .into_iter()
            .filter(|url| !url.trim().is_empty())
            .collect();
        if existing
            .len()
            .saturating_add(image_data.len())
            .saturating_add(pool_image_ids.len())
            .saturating_add(urls.len())
            > domain::tipcard::MAX_CARD_IMAGES
        {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                "A tipcard can have at most 4 images".to_string(),
            ));
        }
        let mut incoming: Vec<image_store::IncomingImage> = image_data
            .into_iter()
            .map(image_store::IncomingImage::DataUrl)
            .collect();

        for pool_id in pool_image_ids {
            let image = image_pool::find_pool_image(&state.db, user_id, pool_id)
                .await
                .map_err(|err| err.into_status_body())?
                .ok_or_else(|| {
                    (
                        axum::http::StatusCode::NOT_FOUND,
                        "Pool image not found".to_string(),
                    )
                })?;
            let bytes = tokio::fs::read(state.image_dir.join(image.storage_path))
                .await
                .map_err(|_| {
                    (
                        axum::http::StatusCode::NOT_FOUND,
                        "Pool image file not found".to_string(),
                    )
                })?;
            incoming.push(image_store::IncomingImage::Bytes {
                bytes,
                mime_type: image.mime_type,
            });
        }
        for url in urls {
            incoming.push(image_store::download_remote_image(&url).await?);
        }
        image_store::append_card_images(&state.db, &state.image_dir, user_id, card_id, incoming)
            .await
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
