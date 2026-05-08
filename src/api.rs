use axum::{
    body::Bytes,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    config::SettingsPatch,
    context,
    db::repositories::{tipcards, token_usage, topics},
    domain, llm, AppState,
};

pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/denpie.rs"));
}

fn protobuf_response<T: Message>(msg: &T) -> Response {
    let mut buf = bytes::BytesMut::with_capacity(msg.encoded_len());
    msg.encode(&mut buf).unwrap();
    (
        [(header::CONTENT_TYPE, "application/x-protobuf")],
        buf.freeze(),
    )
        .into_response()
}

fn empty_response() -> pb::ApiResponse {
    pb::ApiResponse {
        result: Some(pb::api_response::Result::Ok(pb::Empty {})),
    }
}

async fn require_api_key(state: &AppState, api_key: &str) -> Result<(), (StatusCode, String)> {
    state
        .api_keys
        .verify(api_key)
        .await
        .map(|_| ())
        .map_err(|err| err.into_status_body())
}

async fn create_raw_api_key(
    state: &AppState,
    client_name: Option<String>,
) -> Result<String, (StatusCode, String)> {
    state
        .api_keys
        .create(client_name)
        .await
        .map_err(|err| err.into_status_body())
}

fn current_settings(state: &AppState) -> pb::Settings {
    let settings = state.settings.get_settings().unwrap_or_default();

    pb::Settings {
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
        autoupdate_enabled: settings.autoupdate_enabled,
        autoupdate_repo: settings.autoupdate_repo,
        autoupdate_branch: settings.autoupdate_branch,
        autoupdate_check_interval_secs: settings.autoupdate_check_interval_secs,
        autoupdate_command: settings.autoupdate_command,
        autoupdate_last_seen_sha: settings.autoupdate_last_seen_sha,
        daily_time_zone: settings.daily_time_zone,
        daily_update_time: settings.daily_update_time,
        max_active_cards: settings.max_active_cards,
    }
}

fn update_settings_file(
    state: &AppState,
    req: pb::UpdateSettingsRequest,
) -> Result<(), (StatusCode, String)> {
    state
        .settings
        .update_settings(SettingsPatch {
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
            autoupdate_enabled: req.autoupdate_enabled,
            autoupdate_repo: req.autoupdate_repo,
            autoupdate_branch: req.autoupdate_branch,
            autoupdate_check_interval_secs: req.autoupdate_check_interval_secs,
            autoupdate_command: req.autoupdate_command,
            daily_time_zone: req.daily_time_zone,
            daily_update_time: req.daily_update_time,
            max_active_cards: req.max_active_cards,
            ..Default::default()
        })
        .map(|_| ())
        .map_err(|err| err.into_status_body())
}

#[derive(Clone)]
struct TopicClassInfo {
    id: i64,
    name: String,
    tipcard_type: String,
}

#[derive(Clone)]
struct TopicInfo {
    id: i64,
    prompt_template: Option<String>,
    daily_card_count: Option<i64>,
    daily_time_zone: Option<String>,
    daily_update_time: Option<String>,
    compression_level: Option<String>,
}

impl domain::scheduling::DailyWindowTopic for TopicInfo {
    fn daily_card_count(&self) -> Option<i64> {
        self.daily_card_count
    }

    fn daily_time_zone(&self) -> Option<&str> {
        self.daily_time_zone.as_deref()
    }

    fn daily_update_time(&self) -> Option<&str> {
        self.daily_update_time.as_deref()
    }
}

#[derive(Clone, Deserialize)]
pub struct TipsJsonRequest {
    pub count: Option<u32>,
    pub topics: String,
    pub topic_class: Option<String>,
    pub tipcard_type: Option<String>,
    pub exclude_card_ids: Option<Vec<i64>>,
    pub manual_content: Option<String>,
    pub manual_compressed_content: Option<String>,
    pub manual_image_data: Option<Vec<String>>,
}

#[derive(Clone, Serialize)]
pub struct TipCardJson {
    pub id: i64,
    pub topic: String,
    pub full_content: String,
    pub compressed_content: String,
    pub image_data: Vec<String>,
    pub topic_class: String,
    pub tipcard_type: String,
    pub pinned: bool,
}

#[derive(Deserialize)]
pub struct ReviewJsonRequest {
    pub card_id: i64,
    pub grade: Option<u8>,
    pub action: Option<String>,
}

#[derive(Deserialize)]
pub struct ForceDailyRefreshRequest {
    pub topics: String,
    pub topic_class: Option<String>,
    pub tipcard_type: Option<String>,
}

#[derive(Serialize)]
pub struct ForceDailyRefreshResponse {
    pub refreshed_cards: u64,
}

async fn list_api_keys_pb(state: &AppState) -> Result<pb::ApiKeys, (StatusCode, String)> {
    let rows = state
        .api_keys
        .list()
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(pb::ApiKeys {
        keys: rows
            .into_iter()
            .map(|row| pb::ApiKeyInfo {
                id: row.id,
                client_name: row.client_name,
                created_at: row.created_at,
            })
            .collect(),
    })
}

async fn delete_api_key_by_id(state: &AppState, id: i64) -> Result<(), (StatusCode, String)> {
    state
        .api_keys
        .delete(id)
        .await
        .map_err(|err| err.into_status_body())
}

async fn list_admin_topics_pb(state: &AppState) -> Result<pb::AdminTopics, (StatusCode, String)> {
    let rows = topics::list_admin(&state.db)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(pb::AdminTopics {
        topics: rows
            .into_iter()
            .map(|row| pb::AdminTopic {
                id: row.id,
                name: row.name,
                prompt_template: row.prompt_template.unwrap_or_default(),
                daily_card_count: row.daily_card_count.unwrap_or(1).max(1) as u32,
                daily_time_zone: row.daily_time_zone.unwrap_or_default(),
                daily_update_time: row.daily_update_time.unwrap_or_default(),
                compression_level: row.compression_level.unwrap_or_default(),
            })
            .collect(),
    })
}

async fn delete_tipcard_by_id(state: &AppState, id: i64) -> Result<(), (StatusCode, String)> {
    tipcards::delete_with_review(&state.db, id)
        .await
        .map_err(|err| err.into_status_body())
}

pub async fn set_tipcard_pinned(
    state: &AppState,
    id: i64,
    pinned: bool,
) -> Result<(), (StatusCode, String)> {
    tipcards::set_pinned(&state.db, id, pinned)
        .await
        .map_err(|err| err.into_status_body())
}

pub async fn delete_topic_by_id(state: &AppState, id: i64) -> Result<(), (StatusCode, String)> {
    topics::delete_cascade(&state.db, id)
        .await
        .map_err(|err| err.into_status_body())
}

async fn record_llm_token_usage(
    state: &AppState,
    model: &str,
    purpose: &str,
    usage: &llm::TokenUsage,
) -> Result<(), (StatusCode, String)> {
    token_usage::insert(&state.db, model, purpose, usage)
        .await
        .map_err(|err| err.into_status_body())
}

pub async fn set_tipcard_images(
    state: &AppState,
    id: i64,
    image_data: Vec<String>,
) -> Result<(), (StatusCode, String)> {
    let image_data = validate_image_data(image_data)?;
    let image_data_json = image_data_json(&image_data)?;
    tipcards::set_images(&state.db, id, image_data_json)
        .await
        .map_err(|err| err.into_status_body())
}

async fn list_tipcards_pb(state: &AppState) -> Result<pb::Tipcards, (StatusCode, String)> {
    let rows = tipcards::list_admin(&state.db)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(pb::Tipcards {
        cards: rows
            .into_iter()
            .map(|row| pb::TipcardInfo {
                id: row.id,
                topic_name: row.topic_name,
                full_content: row.full_content,
                compressed_content: row.compressed_content,
                created_at: row.created_at,
                tipcard_type: row.tipcard_type,
                topic_class: row.topic_class,
                status: row.status,
                next_review_at: row.next_review_at,
                repeat_count: serde_json::from_str::<serde_json::Value>(&row.state_data)
                    .ok()
                    .and_then(|value| value.get("repeats").and_then(|repeats| repeats.as_u64()))
                    .unwrap_or(0) as u32,
                pinned: row.pinned,
            })
            .collect(),
    })
}

async fn app_summary_pb(state: &AppState) -> Result<pb::AppSummary, (StatusCode, String)> {
    let summary = topics::app_summary(&state.db, Utc::now())
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(pb::AppSummary {
        topics: summary.topics,
        total_cards: summary.total_cards,
        due_cards: summary.due_cards,
        active_cards: summary.active_cards,
    })
}

async fn app_topics_pb(state: &AppState) -> Result<pb::AppTopics, (StatusCode, String)> {
    let rows = topics::list_app_topics(&state.db, Utc::now())
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(pb::AppTopics {
        topics: rows
            .into_iter()
            .map(|row| pb::AppTopicInfo {
                id: row.id,
                name: row.name,
                class_name: row.class_name,
                tipcard_type: row.tipcard_type,
                prompt_template: row.prompt_template,
                daily_card_count: row.daily_card_count,
                daily_time_zone: row.daily_time_zone,
                daily_update_time: row.daily_update_time,
                compression_level: row.compression_level,
                total_cards: row.total_cards,
                due_cards: row.due_cards,
                completed_cards: row.completed_cards,
            })
            .collect(),
    })
}

async fn update_topic_prompt(
    state: &AppState,
    req: pb::UpdateTopicRequest,
) -> Result<(), (StatusCode, String)> {
    let current = topics::get_settings(&state.db, req.id)
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
    .map_err(|err| err.into_status_body())
}

impl From<topics::TopicClassRecord> for TopicClassInfo {
    fn from(record: topics::TopicClassRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            tipcard_type: record.tipcard_type,
        }
    }
}

impl From<topics::TopicRecord> for TopicInfo {
    fn from(record: topics::TopicRecord) -> Self {
        Self {
            id: record.id,
            prompt_template: record.prompt_template,
            daily_card_count: record.daily_card_count,
            daily_time_zone: record.daily_time_zone,
            daily_update_time: record.daily_update_time,
            compression_level: record.compression_level,
        }
    }
}

async fn get_or_create_topic_class(
    state: &AppState,
    class_name: &str,
    requested_type: &str,
) -> Result<TopicClassInfo, (StatusCode, String)> {
    topics::get_or_create_class(&state.db, class_name, requested_type)
        .await
        .map(Into::into)
        .map_err(|err| err.into_status_body())
}

async fn get_or_create_topic(
    state: &AppState,
    topic_name: &str,
    class_id: i64,
) -> Result<TopicInfo, (StatusCode, String)> {
    topics::get_or_create_topic(&state.db, topic_name, class_id)
        .await
        .map(Into::into)
        .map_err(|err| err.into_status_body())
}

fn tip_response_json(
    id: i64,
    topic: &str,
    full_content: String,
    compressed_content: String,
    image_data: Vec<String>,
    class_info: &TopicClassInfo,
    pinned: bool,
) -> TipCardJson {
    TipCardJson {
        id,
        topic: topic.to_string(),
        full_content,
        compressed_content,
        image_data,
        topic_class: class_info.name.clone(),
        tipcard_type: class_info.tipcard_type.clone(),
        pinned,
    }
}

fn parse_image_data(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn image_data_json(images: &[String]) -> Result<String, (StatusCode, String)> {
    serde_json::to_string(images).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub fn validate_image_data(images: Vec<String>) -> Result<Vec<String>, (StatusCode, String)> {
    domain::tipcard::validate_image_data(images)
}

async fn active_card_count(state: &AppState) -> Result<i64, (StatusCode, String)> {
    tipcards::active_card_count(&state.db)
        .await
        .map_err(|err| err.into_status_body())
}

async fn active_card_room(
    state: &AppState,
    max_active_cards: u64,
) -> Result<Option<usize>, (StatusCode, String)> {
    if max_active_cards == 0 {
        return Ok(None);
    }
    let active = active_card_count(state).await?.max(0) as u64;
    Ok(Some(max_active_cards.saturating_sub(active) as usize))
}

pub async fn build_tips(
    state: &AppState,
    query: TipsJsonRequest,
) -> Result<Vec<TipCardJson>, (StatusCode, String)> {
    let count = query.count.unwrap_or(1).max(1);
    let topics: Vec<&str> = query.topics.split(',').collect();
    let mut responses = Vec::new();
    let topic_class = query.topic_class.unwrap_or_default();
    let tipcard_type = query.tipcard_type.unwrap_or_default();
    let manual_content = query.manual_content.unwrap_or_default().trim().to_string();
    let manual_compressed_content = query
        .manual_compressed_content
        .unwrap_or_default()
        .trim()
        .to_string();
    let manual_image_data = validate_image_data(query.manual_image_data.unwrap_or_default())?;
    let exclude_card_ids: Vec<i64> = query
        .exclude_card_ids
        .unwrap_or_default()
        .into_iter()
        .filter(|id| *id > 0)
        .collect();
    let class_info = get_or_create_topic_class(state, &topic_class, &tipcard_type).await?;
    if class_info.tipcard_type == "custom_tip" {
        return Err((
            StatusCode::BAD_REQUEST,
            "custom_tip cards must be submitted with submit_custom_tipcard".to_string(),
        ));
    }

    let settings = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    let llm_model = settings.llm_model.clone();
    let template = settings.prompt_template.clone();
    let llm_api_key = settings.llm_api_key.clone();
    let llm_base_url = settings.llm_base_url.clone();
    let llm_compress_model = settings.llm_compress_model.clone();
    let llm_compress_base_url = settings.llm_compress_base_url.clone();
    let llm_reasoning_effort = settings.llm_reasoning_effort.clone();
    let llm_compression_level =
        llm::CompressionLevel::from_setting(&settings.llm_compression_level);
    let llm_compress_reasoning_effort = settings.llm_compress_reasoning_effort.clone();
    let max_active_cards = settings.max_active_cards;
    let mut active_room = active_card_room(state, max_active_cards).await?;
    let llm_reasoning = llm::ReasoningConfig::new(llm_reasoning_effort);
    let llm_compress_reasoning = llm::ReasoningConfig::new(llm_compress_reasoning_effort);

    for topic_name in topics.into_iter().take(count as usize) {
        let topic_name = topic_name.trim();
        if topic_name.is_empty() {
            continue;
        }

        let topic = get_or_create_topic(state, topic_name, class_info.id).await?;
        if class_info.tipcard_type == "manual_tip" {
            if manual_content.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "manual_content is required for manual_tip".to_string(),
                ));
            }
            if matches!(active_room, Some(0)) {
                return Err((StatusCode::CONFLICT, "Max active cards reached".to_string()));
            }
            let compact = if manual_compressed_content.is_empty() {
                manual_content.clone()
            } else {
                manual_compressed_content.clone()
            };
            create_manual_tipcard(
                state,
                topic_name,
                &topic,
                &class_info,
                manual_content.clone(),
                compact,
                manual_image_data.clone(),
                &mut responses,
            )
            .await?;
            if let Some(room) = active_room.as_mut() {
                *room = room.saturating_sub(1);
            }
            continue;
        }

        let daily_card_count = if is_queue_tipcard(&class_info.tipcard_type) {
            1
        } else {
            domain::scheduling::topic_daily_card_count(&topic)
        };

        let due_cards = tipcards::find_due_topic_cards(
            &state.db,
            topic.id,
            &class_info.tipcard_type,
            &exclude_card_ids,
            daily_card_count,
        )
        .await
        .map_err(|err| err.into_status_body())?;
        for card in &due_cards {
            responses.push(tip_response_json(
                card.id,
                topic_name,
                card.full_content.clone(),
                card.compressed_content.clone(),
                parse_image_data(&card.image_data),
                &class_info,
                card.pinned,
            ));
        }
        if !due_cards.is_empty() {
            continue;
        } else if !is_queue_tipcard(&class_info.tipcard_type) {
            let daily_window_start = domain::scheduling::topic_daily_window_start(
                &topic,
                &settings.daily_time_zone,
                &settings.daily_update_time,
            );
            let daily_cards = tipcards::find_daily_topic_cards(
                &state.db,
                topic.id,
                &class_info.tipcard_type,
                daily_window_start,
                &exclude_card_ids,
                daily_card_count,
            )
            .await
            .map_err(|err| err.into_status_body())?;
            for card in &daily_cards {
                responses.push(tip_response_json(
                    card.id,
                    topic_name,
                    card.full_content.clone(),
                    card.compressed_content.clone(),
                    parse_image_data(&card.image_data),
                    &class_info,
                    card.pinned,
                ));
            }
            let remaining_daily_cards = daily_card_count.saturating_sub(daily_cards.len());
            let cards_to_generate = active_room.map_or(remaining_daily_cards, |room| {
                remaining_daily_cards.min(room)
            });
            for _ in 0..cards_to_generate {
                generate_tipcard(
                    state,
                    topic_name,
                    &topic,
                    &class_info,
                    &template,
                    &llm_model,
                    &llm_api_key,
                    &llm_base_url,
                    &llm_reasoning,
                    &llm_compress_model,
                    &llm_compress_base_url,
                    llm_compression_level,
                    &llm_compress_reasoning,
                    &mut responses,
                )
                .await?;
                if let Some(room) = active_room.as_mut() {
                    *room = room.saturating_sub(1);
                }
            }
        } else if active_room.map_or(true, |room| room > 0) {
            generate_tipcard(
                state,
                topic_name,
                &topic,
                &class_info,
                &template,
                &llm_model,
                &llm_api_key,
                &llm_base_url,
                &llm_reasoning,
                &llm_compress_model,
                &llm_compress_base_url,
                llm_compression_level,
                &llm_compress_reasoning,
                &mut responses,
            )
            .await?;
            if let Some(room) = active_room.as_mut() {
                *room = room.saturating_sub(1);
            }
        }
    }

    Ok(responses)
}

pub async fn force_daily_refresh(
    state: &AppState,
    req: ForceDailyRefreshRequest,
) -> Result<ForceDailyRefreshResponse, (StatusCode, String)> {
    let topic_names: Vec<String> = req
        .topics
        .split(',')
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
        .map(str::to_string)
        .collect();
    let topic_class = req.topic_class.unwrap_or_default();
    let tipcard_type = req.tipcard_type.unwrap_or_default();
    let all_generated_topics =
        topic_names.is_empty() && topic_class.trim().is_empty() && tipcard_type.trim().is_empty();

    let targets: Vec<(TopicInfo, String)> = if all_generated_topics {
        topics::list_generated_targets(&state.db)
            .await
            .map_err(|err| err.into_status_body())?
            .into_iter()
            .map(|(topic, tipcard_type)| (topic.into(), tipcard_type))
            .collect()
    } else {
        let class_info = get_or_create_topic_class(state, &topic_class, &tipcard_type).await?;
        if !domain::tipcard::TipcardType::from_setting(&class_info.tipcard_type).is_generated() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Only generated daily cards can be force-refreshed".to_string(),
            ));
        }
        let mut targets = Vec::new();
        for topic_name in topic_names {
            let topic = get_or_create_topic(state, &topic_name, class_info.id).await?;
            targets.push((topic, class_info.tipcard_type.clone()));
        }
        targets
    };

    let _target_count = targets.len();
    Ok(ForceDailyRefreshResponse { refreshed_cards: 0 })
}

async fn generate_tipcard(
    state: &AppState,
    topic_name: &str,
    topic: &TopicInfo,
    class_info: &TopicClassInfo,
    template: &str,
    llm_model: &str,
    llm_api_key: &str,
    llm_base_url: &str,
    llm_reasoning: &llm::ReasoningConfig,
    llm_compress_model: &str,
    llm_compress_base_url: &str,
    llm_compression_level: llm::CompressionLevel,
    _llm_compress_reasoning: &llm::ReasoningConfig,
    responses: &mut Vec<TipCardJson>,
) -> Result<(), (StatusCode, String)> {
    let template = topic
        .prompt_template
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(template);
    let card_context =
        context::load_card_context(state, topic.id, &class_info.tipcard_type).await?;
    let prompt = context::render_generation_prompt(topic_name, template, &card_context);
    let full_res =
        llm::generate_new_card(llm_model, &prompt, llm_api_key, llm_base_url, llm_reasoning).await;
    record_llm_token_usage(state, llm_model, "generate_card", &full_res.usage).await?;
    let full_tip = full_res.content;
    let compression_level = topic
        .compression_level
        .as_deref()
        .map(llm::CompressionLevel::from_setting)
        .unwrap_or(llm_compression_level);
    let compression_reasoning = llm::ReasoningConfig::new(compression_level.reasoning_effort());

    let compressed_res = llm::compress_card(
        &full_tip,
        llm_compress_model,
        llm_api_key,
        llm_compress_base_url,
        compression_level,
        &compression_reasoning,
    )
    .await;
    record_llm_token_usage(
        state,
        llm_compress_model,
        "compress_card",
        &compressed_res.usage,
    )
    .await?;
    let compressed_tip = compressed_res.content;

    let title_res = llm::generate_card_title(
        &full_tip,
        llm_compress_model,
        llm_api_key,
        llm_compress_base_url,
        &compression_reasoning,
    )
    .await;
    record_llm_token_usage(
        state,
        llm_compress_model,
        "generate_title",
        &title_res.usage,
    )
    .await?;
    let card_title = title_res.content;

    let card_id = tipcards::create_generated(
        &state.db,
        topic.id,
        &class_info.tipcard_type,
        &card_title,
        &full_tip,
        &compressed_tip,
    )
    .await
    .map_err(|err| err.into_status_body())?;

    responses.push(tip_response_json(
        card_id,
        topic_name,
        full_tip,
        compressed_tip,
        Vec::new(),
        class_info,
        false,
    ));

    Ok(())
}

async fn create_manual_tipcard(
    state: &AppState,
    topic_name: &str,
    topic: &TopicInfo,
    class_info: &TopicClassInfo,
    full_tip: String,
    compressed_tip: String,
    image_data: Vec<String>,
    responses: &mut Vec<TipCardJson>,
) -> Result<(), (StatusCode, String)> {
    let title = full_tip
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Manual card")
        .chars()
        .take(96)
        .collect::<String>();

    let image_data_json = image_data_json(&image_data)?;
    let card_id = tipcards::create_manual(
        &state.db,
        topic.id,
        &class_info.tipcard_type,
        &title,
        &full_tip,
        &compressed_tip,
        &image_data_json,
    )
    .await
    .map_err(|err| err.into_status_body())?;

    responses.push(tip_response_json(
        card_id,
        topic_name,
        full_tip,
        compressed_tip,
        image_data,
        class_info,
        false,
    ));

    Ok(())
}

async fn create_custom_tipcard(
    state: &AppState,
    req: pb::CustomTipcardRequest,
) -> Result<TipCardJson, (StatusCode, String)> {
    let topic_name = req.topic.trim();
    if topic_name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "topic is required".to_string()));
    }

    let full_tip = req.full_content.trim().to_string();
    if full_tip.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "full_content is required".to_string(),
        ));
    }

    let compressed_tip = req.compressed_content.trim().to_string();
    let compressed_tip = if compressed_tip.is_empty() {
        full_tip.clone()
    } else {
        compressed_tip
    };
    let title = req.title.trim().to_string();
    let title = if title.is_empty() {
        full_tip
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("Custom card")
            .chars()
            .take(96)
            .collect::<String>()
    } else {
        title.chars().take(96).collect::<String>()
    };

    let class_info = get_or_create_topic_class(state, "custom", "custom_tip").await?;
    let topic = get_or_create_topic(state, topic_name, class_info.id).await?;
    let card_id = tipcards::create_custom(&state.db, topic.id, &title, &full_tip, &compressed_tip)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(tip_response_json(
        card_id,
        topic_name,
        full_tip,
        compressed_tip,
        Vec::new(),
        &class_info,
        false,
    ))
}

pub async fn unified_api(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Response, (StatusCode, String)> {
    let request =
        pb::ApiRequest::decode(body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let op = request
        .op
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Missing API operation".to_string()))?;

    let response = match op {
        pb::api_request::Op::BootstrapApiKey(req) => {
            let settings = state
                .settings
                .get_settings()
                .map_err(|err| err.into_status_body())?;
            if settings.admin_token.is_empty() || req.admin_token != settings.admin_token {
                return Err((StatusCode::UNAUTHORIZED, "Invalid admin token".to_string()));
            }
            let api_key = create_raw_api_key(&state, Some(req.client_name)).await?;
            pb::ApiResponse {
                result: Some(pb::api_response::Result::ApiKeyCreated(pb::ApiKeyCreated {
                    api_key,
                })),
            }
        }
        other => {
            require_api_key(&state, &request.auth).await?;
            match other {
                pb::api_request::Op::Tips(query) => {
                    let responses = build_tips(
                        &state,
                        TipsJsonRequest {
                            count: Some(query.count as u32),
                            topics: query.topics,
                            topic_class: Some(query.topic_class),
                            tipcard_type: Some(query.tipcard_type),
                            exclude_card_ids: Some(query.exclude_card_ids),
                            manual_content: Some(query.manual_content),
                            manual_compressed_content: Some(query.manual_compressed_content),
                            manual_image_data: None,
                        },
                    )
                    .await?
                    .into_iter()
                    .map(|card| pb::TipCardResponse {
                        id: card.id,
                        topic: card.topic,
                        full_content: card.full_content,
                        compressed_content: card.compressed_content,
                        topic_class: card.topic_class,
                        tipcard_type: card.tipcard_type,
                        pinned: card.pinned,
                    })
                    .collect();
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::Tips(pb::TipsResponse {
                            tips: responses,
                        })),
                    }
                }
                pb::api_request::Op::SubmitCustomTipcard(req) => {
                    let card = create_custom_tipcard(&state, req).await?;
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::Tips(pb::TipsResponse {
                            tips: vec![pb::TipCardResponse {
                                id: card.id,
                                topic: card.topic,
                                full_content: card.full_content,
                                compressed_content: card.compressed_content,
                                topic_class: card.topic_class,
                                tipcard_type: card.tipcard_type,
                                pinned: card.pinned,
                            }],
                        })),
                    }
                }
                pb::api_request::Op::ForceDailyRefresh(req) => {
                    let result = force_daily_refresh(
                        &state,
                        ForceDailyRefreshRequest {
                            topics: req.topics,
                            topic_class: Some(req.topic_class),
                            tipcard_type: Some(req.tipcard_type),
                        },
                    )
                    .await?;
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::ForceDailyRefresh(
                            pb::ForceDailyRefreshResponse {
                                refreshed_cards: result.refreshed_cards,
                            },
                        )),
                    }
                }
                pb::api_request::Op::Review(payload) => {
                    apply_review(
                        &state,
                        payload.card_id,
                        payload.grade as u8,
                        &payload.action,
                    )
                    .await?;
                    empty_response()
                }
                pb::api_request::Op::GetTopics(_) => {
                    let rows = topics::list_names(&state.db)
                        .await
                        .map_err(|err| err.into_status_body())?;
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::Topics(pb::GetTopicsResponse {
                            topics: rows,
                        })),
                    }
                }
                pb::api_request::Op::GetTopicClasses(_) => {
                    let rows = topics::list_classes(&state.db)
                        .await
                        .map_err(|err| err.into_status_body())?;
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::TopicClasses(
                            pb::GetTopicClassesResponse {
                                classes: rows
                                    .into_iter()
                                    .map(|row| pb::TopicClass {
                                        id: row.id,
                                        name: row.name,
                                        tipcard_type: row.tipcard_type,
                                    })
                                    .collect(),
                            },
                        )),
                    }
                }
                pb::api_request::Op::GetSettings(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::Settings(current_settings(&state))),
                },
                pb::api_request::Op::UpdateSettings(req) => {
                    update_settings_file(&state, req)?;
                    empty_response()
                }
                pb::api_request::Op::CreateApiKey(req) => {
                    let api_key = create_raw_api_key(&state, Some(req.client_name)).await?;
                    pb::ApiResponse {
                        result: Some(pb::api_response::Result::ApiKeyCreated(pb::ApiKeyCreated {
                            api_key,
                        })),
                    }
                }
                pb::api_request::Op::ListApiKeys(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::ApiKeys(
                        list_api_keys_pb(&state).await?,
                    )),
                },
                pb::api_request::Op::DeleteApiKey(req) => {
                    delete_api_key_by_id(&state, req.id).await?;
                    empty_response()
                }
                pb::api_request::Op::ListAdminTopics(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::AdminTopics(
                        list_admin_topics_pb(&state).await?,
                    )),
                },
                pb::api_request::Op::ListTipcards(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::Tipcards(
                        list_tipcards_pb(&state).await?,
                    )),
                },
                pb::api_request::Op::DeleteTipcard(req) => {
                    delete_tipcard_by_id(&state, req.id).await?;
                    empty_response()
                }
                pb::api_request::Op::PinTipcard(req) => {
                    set_tipcard_pinned(&state, req.id, req.pinned).await?;
                    empty_response()
                }
                pb::api_request::Op::DeleteTopic(req) => {
                    delete_topic_by_id(&state, req.id).await?;
                    empty_response()
                }
                pb::api_request::Op::GetSummary(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::Summary(
                        app_summary_pb(&state).await?,
                    )),
                },
                pb::api_request::Op::ListAppTopics(_) => pb::ApiResponse {
                    result: Some(pb::api_response::Result::AppTopics(
                        app_topics_pb(&state).await?,
                    )),
                },
                pb::api_request::Op::UpdateTopic(req) => {
                    update_topic_prompt(&state, req).await?;
                    empty_response()
                }
                pb::api_request::Op::BootstrapApiKey(_) => unreachable!(),
            }
        }
    };

    Ok(protobuf_response(&response))
}

pub async fn apply_review(
    state: &AppState,
    card_id: i64,
    grade: u8,
    action: &str,
) -> Result<(), (StatusCode, String)> {
    state
        .reviews
        .apply_review(card_id, grade, action)
        .await
        .map_err(|err| err.into_status_body())
}

fn is_queue_tipcard(tipcard_type: &str) -> bool {
    domain::tipcard::is_queue_tipcard(tipcard_type)
}
