use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use tower_sessions::Session;

use crate::AppState;
use crate::config;
use crate::dashboard::response::{SettingsRes, UpdateSettingsReq};
use crate::dashboard::util::{current_user, settings_response};
use crate::services::settings::SettingsService;

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
        compress_model: req.compress_model,
        template: req.template,
        api_key: req.api_key,
        base_url: req.base_url,
        compress_base_url: req.compress_base_url,
        reasoning_effort: req.reasoning_effort,
        compress_reasoning_effort: req.compress_reasoning_effort,
        compression_level: req.compression_level,
        color_scheme: req.color_scheme,
        transparency: req.transparency,
        blur_intensity: req.blur_intensity,
        ui_blur: req.ui_blur,
        autoupdate_enabled: req.autoupdate_enabled,
        autoupdate_repo: req.autoupdate_repo,
        autoupdate_branch: req.autoupdate_branch,
        autoupdate_check_interval_secs: req.autoupdate_check_interval_secs,
        autoupdate_command: req.autoupdate_command,
        daily_time_zone: req.daily_time_zone,
        daily_update_time: req.daily_update_time,
        max_active_cards: req.max_active_cards,
    };
    if user.role == "admin"
        && (patch.autoupdate_enabled.is_some()
            || patch.autoupdate_repo.is_some()
            || patch.autoupdate_branch.is_some()
            || patch.autoupdate_check_interval_secs.is_some()
            || patch.autoupdate_command.is_some())
    {
        state
            .settings
            .update_settings(config::SettingsPatch {
                autoupdate_enabled: patch.autoupdate_enabled,
                autoupdate_repo: patch.autoupdate_repo.clone(),
                autoupdate_branch: patch.autoupdate_branch.clone(),
                autoupdate_check_interval_secs: patch.autoupdate_check_interval_secs,
                autoupdate_command: patch.autoupdate_command.clone(),
                ..Default::default()
            })
            .map_err(|err| err.into_status_body())?;
    }
    let current = SettingsService::user_settings_get(&state, &user.id)
        .await
        .map_err(|err| err.into_status_body())?;
    let updated = current.apply_patch(patch);
    SettingsService::user_settings_upsert(&state, &user.id, &updated)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(Json(()))
}
