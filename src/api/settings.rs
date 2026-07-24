use crate::{AppState, config::SettingsPatch, db::repositories::user_settings};

use super::{pb, types::ApiResult};

pub(crate) async fn current_settings(state: &AppState, user_id: &str) -> ApiResult<pb::Settings> {
    let defaults = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    let settings = user_settings::get(&state.db, user_id, defaults)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(pb::Settings {
        model: settings.llm_model,
        grounding_model: settings.llm_grounding_model,
        compress_model: settings.llm_compress_model,
        template: settings.prompt_template,
        api_key: settings.llm_api_key,
        base_url: settings.llm_base_url,
        compress_base_url: settings.llm_compress_base_url,
        reasoning_effort: settings.llm_reasoning_effort,
        grounding_reasoning_effort: settings.llm_grounding_reasoning_effort,
        compress_reasoning_effort: settings.llm_compress_reasoning_effort,
        compression_level: settings.llm_compression_level,
        color_scheme: settings.color_scheme,
        autoupdate_enabled: settings.autoupdate_enabled,
        autoupdate_repo: settings.autoupdate_repo,
        autoupdate_branch: settings.autoupdate_branch,
        autoupdate_check_interval_secs: settings.autoupdate_check_interval_secs,
        autoupdate_command: settings.autoupdate_command,
        autoupdate_last_seen_sha: settings.autoupdate_last_seen_sha,
        daily_time_zone: settings.daily_time_zone,
        daily_update_time: settings.daily_update_time,
        max_active_cards: settings.max_active_cards,
        grounding_strategy: settings.grounding_strategy,
        image_strategy: settings.image_strategy,
        search_api_key: settings.search_api_key,
        search_base_url: settings.search_base_url,
        image_sources: settings.image_sources,
    })
}

pub(crate) async fn update_settings_file(
    state: &AppState,
    user_id: &str,
    req: pb::UpdateSettingsRequest,
) -> ApiResult<()> {
    let defaults = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    let current = user_settings::get(&state.db, user_id, defaults)
        .await
        .map_err(|err| err.into_status_body())?;
    let updated = current.apply_patch(SettingsPatch {
        model: req.model,
        grounding_model: req.grounding_model,
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
        grounding_strategy: req.grounding_strategy,
        image_strategy: req.image_strategy,
        search_api_key: req.search_api_key,
        search_base_url: req.search_base_url,
        image_sources: req.image_sources,
        daily_time_zone: req.daily_time_zone,
        daily_update_time: req.daily_update_time,
        max_active_cards: req.max_active_cards,
        ..Default::default()
    });
    user_settings::upsert(&state.db, user_id, &updated)
        .await
        .map_err(|err| err.into_status_body())
}
