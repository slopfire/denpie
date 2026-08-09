use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use tower_sessions::Session;

use crate::AppState;
use crate::config;
use crate::dashboard::response::{SettingsRes, UpdateSettingsReq};
use crate::dashboard::util::{current_user, settings_response};
use crate::services::documents::DocumentService;
use crate::services::settings::SettingsService;

#[derive(serde::Serialize)]
pub struct VisionModelTestRes {
    pub ok: bool,
    pub model: String,
    pub message: String,
}

/// Non-destructive vision connectivity check for the Settings UI.
pub async fn test_vision_model(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Json<VisionModelTestRes>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let result = DocumentService::test_vision_model(&state, &user.id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(VisionModelTestRes {
        ok: result.ok,
        model: result.model,
        message: result.message,
    }))
}

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Json<SettingsRes>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let settings = SettingsService::user_settings_get(&state, &user.id)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(Json(settings_response(settings, user.role == "admin")))
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<UpdateSettingsReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let patch = config::SettingsPatch {
        model: req.model,
        grounding_model: req.grounding_model,
        vision_model: req.vision_model,
        compress_model: req.compress_model,
        template: req.template,
        api_key: req.api_key,
        base_url: req.base_url,
        compress_base_url: req.compress_base_url,
        reasoning_effort: req.reasoning_effort,
        grounding_reasoning_effort: req.grounding_reasoning_effort,
        compress_reasoning_effort: req.compress_reasoning_effort,
        compression_level: req.compression_level,
        color_scheme: None,
        transparency: None,
        blur_intensity: None,
        ui_blur: None,
        autoupdate_enabled: req.autoupdate_enabled,
        autoupdate_repo: req.autoupdate_repo,
        autoupdate_branch: req.autoupdate_branch,
        autoupdate_check_interval_secs: req.autoupdate_check_interval_secs,
        autoupdate_command: req.autoupdate_command,
        daily_time_zone: req.daily_time_zone,
        daily_update_time: req.daily_update_time,
        max_active_cards: req.max_active_cards,
        grounding_strategy: req.grounding_strategy,
        image_strategy: req.image_strategy,
        search_provider: req.search_provider,
        scrape_provider: req.scrape_provider,
        search_api_key: req.search_api_key,
        search_base_url: req.search_base_url,
        image_sources: req.image_sources,
    };
    SettingsService::update_user_settings(&state, &user.id, user.role == "admin", patch)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(Json(()))
}
