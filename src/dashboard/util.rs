use std::sync::Arc;

use axum::http::StatusCode;
use tower_sessions::Session;

use crate::auth::AuthUser;
use crate::dashboard::response::SettingsRes;
use crate::{AppState, auth, config};

pub async fn current_user(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<AuthUser, (StatusCode, String)> {
    auth::current_user(state, session).await
}

pub async fn optional_user(state: &Arc<AppState>, session: &Session) -> Option<AuthUser> {
    auth::current_user(state, session).await.ok()
}

pub async fn require_admin(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<AuthUser, (StatusCode, String)> {
    let user = auth::current_user(state, session).await?;
    if user.role != "admin" {
        return Err((StatusCode::FORBIDDEN, "Admin only".to_string()));
    }
    Ok(user)
}

pub fn settings_response(settings: config::Settings, show_autoupdate: bool) -> SettingsRes {
    SettingsRes {
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        build_sha: option_env!("DENPIE_BUILD_SHA")
            .unwrap_or("unknown")
            .to_string(),
        model: settings.llm_model,
        compress_model: settings.llm_compress_model,
        template: settings.prompt_template,
        api_key: settings.llm_api_key,
        base_url: settings.llm_base_url,
        compress_base_url: settings.llm_compress_base_url,
        reasoning_effort: settings.llm_reasoning_effort,
        compress_reasoning_effort: settings.llm_compress_reasoning_effort,
        compression_level: settings.llm_compression_level,
        color_scheme: settings.color_scheme,
        transparency: settings.transparency,
        blur_intensity: settings.blur_intensity,
        autoupdate_enabled: show_autoupdate && settings.autoupdate_enabled,
        autoupdate_repo: if show_autoupdate {
            settings.autoupdate_repo
        } else {
            String::new()
        },
        autoupdate_branch: if show_autoupdate {
            settings.autoupdate_branch
        } else {
            String::new()
        },
        autoupdate_check_interval_secs: if show_autoupdate {
            settings.autoupdate_check_interval_secs
        } else {
            0
        },
        autoupdate_command: if show_autoupdate {
            settings.autoupdate_command
        } else {
            String::new()
        },
        autoupdate_last_seen_sha: if show_autoupdate {
            settings.autoupdate_last_seen_sha
        } else {
            String::new()
        },
        daily_time_zone: settings.daily_time_zone,
        daily_update_time: settings.daily_update_time,
        max_active_cards: settings.max_active_cards,
    }
}

pub fn parse_flow_cursor(cursor: &str) -> Option<(i64, String, i64)> {
    let mut parts = cursor.splitn(3, '|');
    let pinned = parts.next()?.parse().ok()?;
    let created_at = parts.next()?.to_string();
    let id = parts.next()?.parse().ok()?;
    Some((pinned, created_at, id))
}

pub fn image_url(id: i64) -> String {
    format!("/app/tipcard-images/{id}")
}
