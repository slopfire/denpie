use crate::{
    AppState, auth::AuthUser, config::SettingsPatch,
    services::prompt_enhance::PromptEnhanceService, services::settings::SettingsService,
};

use super::{pb, types::ApiResult};

pub(crate) async fn current_settings(
    state: &AppState,
    user: &AuthUser,
    show_secrets: bool,
) -> ApiResult<pb::Settings> {
    let settings = SettingsService::user_settings_get(state, &user.id)
        .await
        .map_err(|err| err.into_status_body())?;
    let is_admin = user.role == "admin";

    Ok(pb::Settings {
        model: settings.llm_model,
        grounding_model: settings.llm_grounding_model,
        vision_model: settings.llm_vision_model,
        compress_model: settings.llm_compress_model,
        template: settings.prompt_template,
        api_key: if show_secrets {
            settings.llm_api_key
        } else {
            String::new()
        },
        base_url: settings.llm_base_url,
        compress_base_url: settings.llm_compress_base_url,
        reasoning_effort: settings.llm_reasoning_effort,
        grounding_reasoning_effort: settings.llm_grounding_reasoning_effort,
        compress_reasoning_effort: settings.llm_compress_reasoning_effort,
        compression_level: settings.llm_compression_level,
        color_scheme: settings.color_scheme,
        transparency: settings.transparency,
        blur_intensity: settings.blur_intensity,
        autoupdate_enabled: is_admin && settings.autoupdate_enabled,
        autoupdate_repo: if is_admin {
            settings.autoupdate_repo
        } else {
            String::new()
        },
        autoupdate_branch: if is_admin {
            settings.autoupdate_branch
        } else {
            String::new()
        },
        autoupdate_check_interval_secs: if is_admin {
            settings.autoupdate_check_interval_secs
        } else {
            0
        },
        autoupdate_command: if is_admin {
            settings.autoupdate_command
        } else {
            String::new()
        },
        autoupdate_last_seen_sha: if is_admin {
            settings.autoupdate_last_seen_sha
        } else {
            String::new()
        },
        daily_time_zone: settings.daily_time_zone,
        daily_update_time: settings.daily_update_time,
        max_active_cards: settings.max_active_cards,
        grounding_strategy: settings.grounding_strategy,
        image_strategy: settings.image_strategy,
        search_provider: settings.search_provider,
        scrape_provider: settings.scrape_provider,
        search_api_key: if show_secrets {
            settings.search_api_key
        } else {
            String::new()
        },
        search_base_url: settings.search_base_url,
        image_sources: settings.image_sources,
    })
}

pub(crate) async fn enhance_prompt_template(
    state: &AppState,
    user_id: &str,
    topic_id: i64,
) -> ApiResult<pb::EnhancePromptTemplateResult> {
    let suggestion = PromptEnhanceService::suggest(state, user_id, topic_id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(pb::EnhancePromptTemplateResult {
        prompt_template: suggestion.prompt_template,
        grounding_strategy: suggestion.grounding_strategy,
        grounding_model: suggestion.grounding_model,
        grounding_reasoning_effort: suggestion.grounding_reasoning_effort,
        image_strategy: suggestion.image_strategy,
        rationale: suggestion.rationale,
    })
}

pub(crate) async fn update_settings_file(
    state: &AppState,
    user: &AuthUser,
    req: pb::UpdateSettingsRequest,
) -> ApiResult<()> {
    SettingsService::update_user_settings(
        state,
        &user.id,
        user.role == "admin",
        SettingsPatch {
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
            color_scheme: req.color_scheme,
            transparency: req.transparency,
            blur_intensity: req.blur_intensity,
            autoupdate_enabled: req.autoupdate_enabled,
            autoupdate_repo: req.autoupdate_repo,
            autoupdate_branch: req.autoupdate_branch,
            autoupdate_check_interval_secs: req.autoupdate_check_interval_secs,
            autoupdate_command: req.autoupdate_command,
            grounding_strategy: req.grounding_strategy,
            image_strategy: req.image_strategy,
            search_provider: req.search_provider,
            scrape_provider: req.scrape_provider,
            search_api_key: req.search_api_key,
            search_base_url: req.search_base_url,
            image_sources: req.image_sources,
            daily_time_zone: req.daily_time_zone,
            daily_update_time: req.daily_update_time,
            max_active_cards: req.max_active_cards,
            ..Default::default()
        },
    )
    .await
    .map_err(|err| err.into_status_body())
}
