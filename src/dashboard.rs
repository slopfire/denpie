use crate::{
    api, autoupdate, config,
    db::repositories::{tipcards, token_usage, topics, user_settings},
    llm, AppState,
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tower_sessions::Session;

#[derive(Serialize)]
pub struct SettingsRes {
    server_version: String,
    build_sha: String,
    model: String,
    compress_model: String,
    template: String,
    api_key: String,
    base_url: String,
    compress_base_url: String,
    reasoning_effort: String,
    compress_reasoning_effort: String,
    compression_level: String,
    color_scheme: String,
    transparency: String,
    blur_intensity: String,
    autoupdate_enabled: bool,
    autoupdate_repo: String,
    autoupdate_branch: String,
    autoupdate_check_interval_secs: u64,
    autoupdate_command: String,
    autoupdate_last_seen_sha: String,
    daily_time_zone: String,
    daily_update_time: String,
    max_active_cards: u64,
}

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Json<SettingsRes>, (StatusCode, String)> {
    let user = crate::auth::current_user(&state, &session).await?;
    let defaults = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    let mut settings = user_settings::get(&state.db, &user.id, defaults)
        .await
        .map_err(|err| err.into_status_body())?;
    if user.role != "admin" {
        settings.autoupdate_enabled = false;
        settings.autoupdate_command.clear();
    }

    Ok(Json(settings_response(settings, user.role == "admin")))
}

fn settings_response(settings: config::Settings, show_autoupdate: bool) -> SettingsRes {
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

#[derive(Deserialize)]
pub struct UpdateSettingsReq {
    model: Option<String>,
    compress_model: Option<String>,
    template: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    compress_base_url: Option<String>,
    reasoning_effort: Option<String>,
    compress_reasoning_effort: Option<String>,
    compression_level: Option<String>,
    color_scheme: Option<String>,
    transparency: Option<String>,
    blur_intensity: Option<String>,
    ui_blur: Option<String>,
    autoupdate_enabled: Option<bool>,
    autoupdate_repo: Option<String>,
    autoupdate_branch: Option<String>,
    autoupdate_check_interval_secs: Option<u64>,
    autoupdate_command: Option<String>,
    daily_time_zone: Option<String>,
    daily_update_time: Option<String>,
    max_active_cards: Option<u64>,
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<UpdateSettingsReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = crate::auth::current_user(&state, &session).await?;
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
    let defaults = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    let current = user_settings::get(&state.db, &user.id, defaults)
        .await
        .map_err(|err| err.into_status_body())?;
    let updated = current.apply_patch(patch);
    user_settings::upsert(&state.db, &user.id, &updated)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(Json(()))
}

#[derive(Serialize)]
pub struct TriggerAutoupdateRes {
    message: String,
    restarting: bool,
    updating: bool,
    target_sha: Option<String>,
    build_sha: String,
}

pub async fn trigger_autoupdate(State(state): State<Arc<AppState>>, session: Session) -> Response {
    match crate::auth::current_user(&state, &session).await {
        Ok(user) if user.role == "admin" => {}
        Ok(_) => return (StatusCode::FORBIDDEN, "Admin only").into_response(),
        Err(err) => return err.into_response(),
    }
    match autoupdate::trigger_manual(&state.settings_path).await {
        Ok(result) => {
            if result.should_exit_for_restart {
                tokio::spawn(async {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    std::process::exit(75);
                });
            }
            Json(TriggerAutoupdateRes {
                message: result.message,
                restarting: result.should_exit_for_restart,
                updating: result.update_started,
                target_sha: result.target_sha,
                build_sha: option_env!("DENPIE_BUILD_SHA")
                    .unwrap_or("unknown")
                    .to_string(),
            })
            .into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    }
}

pub async fn autoupdate_status(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Json<autoupdate::UpdateStatus> {
    match crate::auth::current_user(&state, &session).await {
        Ok(user) if user.role == "admin" => {}
        _ => {
            return Json(autoupdate::UpdateStatus {
                phase: "forbidden".to_string(),
                message: "Admin only".to_string(),
                target_sha: String::new(),
                updated_at: String::new(),
            })
        }
    }
    Json(autoupdate::read_status(&state.settings_path))
}

#[derive(Deserialize)]
pub struct CreateKeyReq {
    pub client_name: Option<String>,
}

pub async fn create_api_key(
    State(state): State<Arc<AppState>>,
    session: Session,
    req: Option<Json<CreateKeyReq>>,
) -> Json<String> {
    let user = match crate::auth::current_user(&state, &session).await {
        Ok(user) => user,
        Err(_) => return Json(String::new()),
    };
    let client_name = req
        .and_then(|Json(r)| r.client_name)
        .unwrap_or_else(|| "default_client".to_string());
    let api_key = state
        .api_keys
        .create(&user.id, Some(client_name))
        .await
        .unwrap_or_default();

    Json(api_key)
}

#[derive(Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub client_name: String,
    pub created_at: String,
}

pub async fn list_api_keys(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Json<Vec<ApiKeyInfo>> {
    let user = match crate::auth::current_user(&state, &session).await {
        Ok(user) => user,
        Err(_) => return Json(Vec::new()),
    };
    let keys = state
        .api_keys
        .list(&user.id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| ApiKeyInfo {
            id: row.id,
            client_name: row.client_name,
            created_at: row.created_at,
        })
        .collect();

    Json(keys)
}

#[derive(Deserialize)]
pub struct DeleteKeyReq {
    pub id: i64,
}

pub async fn delete_api_key(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<DeleteKeyReq>,
) -> StatusCode {
    let user = match crate::auth::current_user(&state, &session).await {
        Ok(user) => user,
        Err(_) => return StatusCode::UNAUTHORIZED,
    };
    if state.api_keys.delete(&user.id, req.id).await.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Deserialize)]
pub struct DeleteTipcardReq {
    pub id: i64,
}

#[derive(Deserialize)]
pub struct PinTipcardReq {
    pub id: i64,
    pub pinned: Option<bool>,
    pub image_data: Option<Vec<String>>,
}

pub async fn pin_tipcard(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<PinTipcardReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = crate::auth::current_user(&state, &session).await?;
    if let Some(pinned) = req.pinned {
        api::set_tipcard_pinned(&state, &user.id, req.id, pinned).await?;
    }
    if let Some(image_data) = req.image_data {
        api::set_tipcard_images(&state, &user.id, req.id, image_data).await?;
    }
    Ok(Json(()))
}

pub async fn delete_tipcard(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<DeleteTipcardReq>,
) -> StatusCode {
    let user = match crate::auth::current_user(&state, &session).await {
        Ok(user) => user,
        Err(_) => return StatusCode::UNAUTHORIZED,
    };
    match tipcards::delete_with_review(&state.db, &user.id, req.id).await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Serialize)]
pub struct TopicInfo {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
    pub prompt_template: String,
    pub daily_card_count: u32,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub compression_level: String,
}

pub async fn list_topics(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Json<Vec<TopicInfo>> {
    let user = match crate::auth::current_user(&state, &session).await {
        Ok(user) => user,
        Err(_) => return Json(Vec::new()),
    };
    let topics = topics::list_admin(&state.db, &user.id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| TopicInfo {
            id: r.id,
            name: r.name,
            tipcard_type: r.tipcard_type,
            prompt_template: r.prompt_template.unwrap_or_default(),
            daily_card_count: r.daily_card_count.unwrap_or(1).max(1) as u32,
            daily_time_zone: r.daily_time_zone.unwrap_or_default(),
            daily_update_time: r.daily_update_time.unwrap_or_default(),
            compression_level: r.compression_level.unwrap_or_default(),
        })
        .collect();
    Json(topics)
}

#[derive(Serialize)]
pub struct TokenSpend {
    pub daily: i64,
    pub monthly: i64,
    pub total: i64,
}

pub async fn token_spend(State(state): State<Arc<AppState>>, session: Session) -> Json<TokenSpend> {
    let user = match crate::auth::current_user(&state, &session).await {
        Ok(user) => user,
        Err(_) => {
            return Json(TokenSpend {
                daily: 0,
                monthly: 0,
                total: 0,
            })
        }
    };
    match token_usage::aggregate_spend(&state.db, &user.id).await {
        Ok(spend) => Json(TokenSpend {
            daily: spend.daily,
            monthly: spend.monthly,
            total: spend.total,
        }),
        Err(_) => Json(TokenSpend {
            daily: 0,
            monthly: 0,
            total: 0,
        }),
    }
}

#[derive(Serialize)]
pub struct AppSummary {
    pub topics: i64,
    pub total_cards: i64,
    pub due_cards: i64,
    pub active_cards: i64,
}

pub async fn app_summary(State(state): State<Arc<AppState>>, session: Session) -> Json<AppSummary> {
    let user = match crate::auth::current_user(&state, &session).await {
        Ok(user) => user,
        Err(_) => {
            return Json(AppSummary {
                topics: 0,
                total_cards: 0,
                due_cards: 0,
                active_cards: 0,
            })
        }
    };
    match topics::app_summary(&state.db, &user.id, chrono::Utc::now()).await {
        Ok(summary) => Json(AppSummary {
            topics: summary.topics,
            total_cards: summary.total_cards,
            due_cards: summary.due_cards,
            active_cards: summary.active_cards,
        }),
        Err(_) => Json(AppSummary {
            topics: 0,
            total_cards: 0,
            due_cards: 0,
            active_cards: 0,
        }),
    }
}

#[derive(Serialize)]
pub struct AppTopicInfo {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
    pub prompt_template: String,
    pub total_cards: i64,
    pub due_cards: i64,
    pub completed_cards: i64,
    pub daily_card_count: u32,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub compression_level: String,
}

pub async fn app_topics(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Json<Vec<AppTopicInfo>> {
    let user = match crate::auth::current_user(&state, &session).await {
        Ok(user) => user,
        Err(_) => return Json(Vec::new()),
    };
    Json(
        topics::list_app_topics(&state.db, &user.id, chrono::Utc::now())
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| AppTopicInfo {
                id: r.id,
                name: r.name,
                tipcard_type: r.tipcard_type,
                prompt_template: r.prompt_template,
                daily_card_count: r.daily_card_count,
                daily_time_zone: r.daily_time_zone,
                daily_update_time: r.daily_update_time,
                compression_level: r.compression_level,
                total_cards: r.total_cards,
                due_cards: r.due_cards,
                completed_cards: r.completed_cards,
            })
            .collect(),
    )
}

#[derive(Deserialize)]
pub struct UpdateTopicReq {
    pub id: i64,
    pub prompt_template: Option<String>,
    pub daily_card_count: Option<u32>,
    pub daily_time_zone: Option<String>,
    pub daily_update_time: Option<String>,
    pub compression_level: Option<String>,
}

pub async fn update_topic(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<UpdateTopicReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = crate::auth::current_user(&state, &session).await?;
    let current = topics::get_settings(&state.db, &user.id, req.id)
        .await
        .map_err(|err| err.into_status_body())?;

    let prompt_template = req
        .prompt_template
        .map(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        })
        .unwrap_or(current.prompt_template);
    let daily_card_count = req
        .daily_card_count
        .map(|value| {
            if value == 0 {
                None
            } else {
                Some(i64::from(value))
            }
        })
        .unwrap_or(current.daily_card_count);
    let daily_time_zone = req
        .daily_time_zone
        .map(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        })
        .unwrap_or(current.daily_time_zone);
    let daily_update_time = req
        .daily_update_time
        .map(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        })
        .unwrap_or(current.daily_update_time);
    let compression_level = req
        .compression_level
        .map(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(
                    llm::CompressionLevel::from_setting(&value)
                        .as_setting()
                        .to_string(),
                )
            }
        })
        .unwrap_or(current.compression_level);

    topics::update_settings(
        &state.db,
        &user.id,
        req.id,
        topics::TopicSettingsRecord {
            prompt_template,
            daily_card_count,
            daily_time_zone,
            daily_update_time,
            compression_level,
        },
    )
    .await
    .map_err(|err| err.into_status_body())?;

    Ok(Json(()))
}

#[derive(Deserialize)]
pub struct DeleteTopicReq {
    pub id: i64,
}

pub async fn delete_topic(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<DeleteTopicReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = crate::auth::current_user(&state, &session).await?;
    api::delete_topic_by_id(&state, &user.id, req.id).await?;
    Ok(Json(()))
}

#[derive(Serialize, Deserialize)]
pub struct TipcardInfo {
    pub id: i64,
    pub topic_name: String,
    pub title: String,
    pub full_content: String,
    pub compressed_content: String,
    pub image_data: Vec<String>,
    pub created_at: String,
    pub tipcard_type: String,
    pub status: String,
    pub next_review_at: String,
    pub repeat_count: u32,
    pub pinned: bool,
}

#[derive(Default, Deserialize)]
pub struct ListTipcardsQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub topic: Option<String>,
    pub tipcard_type: Option<String>,
}

pub async fn list_tipcards(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<ListTipcardsQuery>,
) -> Json<Vec<TipcardInfo>> {
    let user = match crate::auth::current_user(&state, &session).await {
        Ok(user) => user,
        Err(_) => return Json(Vec::new()),
    };
    let cards = tipcards::list_filtered(
        &state.db,
        &user.id,
        tipcards::TipcardFilter {
            q: query.q,
            status: query.status,
            topic: query.topic,
            tipcard_type: query.tipcard_type,
        },
    )
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| TipcardInfo {
        id: r.id,
        topic_name: r.topic_name,
        title: r.title,
        full_content: r.full_content,
        compressed_content: r.compressed_content,
        image_data: serde_json::from_str::<Vec<String>>(&r.image_data).unwrap_or_default(),
        created_at: r.created_at,
        tipcard_type: r.tipcard_type,
        status: r.status,
        next_review_at: r.next_review_at,
        repeat_count: serde_json::from_str::<serde_json::Value>(&r.state_data)
            .ok()
            .and_then(|value| value.get("repeats").and_then(|repeats| repeats.as_u64()))
            .unwrap_or(0) as u32,
        pinned: r.pinned,
    })
    .collect();

    Json(cards)
}

pub async fn app_tips(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<api::TipsJsonRequest>,
) -> Result<Json<Vec<api::TipCardJson>>, (StatusCode, String)> {
    let user = crate::auth::current_user(&state, &session).await?;
    api::build_tips(&state, &user.id, req).await.map(Json)
}

pub async fn app_review(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<api::ReviewJsonRequest>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = crate::auth::current_user(&state, &session).await?;
    let grade = req.grade.unwrap_or(3).min(5);
    let action = req.action.unwrap_or_default();
    api::apply_review(&state, &user.id, req.card_id, grade, &action).await?;
    Ok(Json(()))
}

pub async fn force_daily_refresh(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<api::ForceDailyRefreshRequest>,
) -> Result<Json<api::ForceDailyRefreshResponse>, (StatusCode, String)> {
    let user = crate::auth::current_user(&state, &session).await?;
    api::force_daily_refresh(&state, &user.id, req)
        .await
        .map(Json)
}
