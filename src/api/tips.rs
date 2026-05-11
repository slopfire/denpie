use axum::http::StatusCode;

use crate::{
    context,
    db::repositories::{daily_refresh, tipcards, token_usage, topics, user_settings, users},
    domain, llm, AppState,
};

use super::{
    pb,
    response::tip_response_json,
    tipcards::{active_card_room, image_data_json, parse_image_data, validate_image_data},
    topics::get_or_create_topic,
    types::{
        ApiResult, ForceDailyRefreshRequest, ForceDailyRefreshResponse, TipCardJson,
        TipsJsonRequest, TopicInfo,
    },
};

pub async fn build_tips(
    state: &AppState,
    user_id: &str,
    query: TipsJsonRequest,
) -> ApiResult<Vec<TipCardJson>> {
    let count = query.count.unwrap_or(1).max(1);
    let topics_list: Vec<&str> = query.topics.split(',').collect();
    let mut responses = Vec::new();
    let requested_type = query
        .tipcard_type
        .unwrap_or_else(|| "repeatable_tip".to_string());

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

    let defaults = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    let settings = user_settings::get(&state.db, user_id, defaults)
        .await
        .map_err(|err| err.into_status_body())?;
    let llm_reasoning = llm::ReasoningConfig::new(settings.llm_reasoning_effort.clone());
    let llm_compress_reasoning =
        llm::ReasoningConfig::new(settings.llm_compress_reasoning_effort.clone());
    let llm_compression_level =
        llm::CompressionLevel::from_setting(&settings.llm_compression_level);
    let mut active_room = active_card_room(state, user_id, settings.max_active_cards).await?;

    for topic_name in topics_list.into_iter().take(count as usize) {
        let topic_name = topic_name.trim();
        if topic_name.is_empty() {
            continue;
        }

        let topic = get_or_create_topic(state, user_id, topic_name, &requested_type).await?;
        if topic.tipcard_type == "custom_tip" {
            return Err((
                StatusCode::BAD_REQUEST,
                "custom_tip cards must be submitted with submit_custom_tipcard".to_string(),
            ));
        }

        if topic.tipcard_type == "manual_tip" {
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
                user_id,
                topic_name,
                &topic,
                manual_content.clone(),
                compact,
                manual_image_data.clone(),
                &mut responses,
            )
            .await?;
            decrement_room(&mut active_room);
            continue;
        }

        let daily_card_count = if is_queue_tipcard(&topic.tipcard_type) {
            1
        } else {
            domain::scheduling::topic_daily_card_count(&topic)
        };

        let due_cards = tipcards::find_due_topic_cards(
            &state.db,
            user_id,
            topic.id,
            &topic.tipcard_type,
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
                topic.tipcard_type.clone(),
                card.pinned,
            ));
        }
        if !due_cards.is_empty() {
            continue;
        } else if !is_queue_tipcard(&topic.tipcard_type) {
            let daily_window_start = domain::scheduling::topic_daily_window_start(
                &topic,
                &settings.daily_time_zone,
                &settings.daily_update_time,
            );
            let daily_cards = tipcards::find_daily_topic_cards(
                &state.db,
                user_id,
                topic.id,
                &topic.tipcard_type,
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
                    topic.tipcard_type.clone(),
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
                    user_id,
                    topic_name,
                    &topic,
                    &settings.prompt_template,
                    &settings.llm_model,
                    &settings.llm_api_key,
                    &settings.llm_base_url,
                    &llm_reasoning,
                    &settings.llm_compress_model,
                    &settings.llm_compress_base_url,
                    llm_compression_level,
                    &llm_compress_reasoning,
                    &mut responses,
                )
                .await?;
                decrement_room(&mut active_room);
            }
        } else if active_room.is_none_or(|room| room > 0) {
            generate_tipcard(
                state,
                user_id,
                topic_name,
                &topic,
                &settings.prompt_template,
                &settings.llm_model,
                &settings.llm_api_key,
                &settings.llm_base_url,
                &llm_reasoning,
                &settings.llm_compress_model,
                &settings.llm_compress_base_url,
                llm_compression_level,
                &llm_compress_reasoning,
                &mut responses,
            )
            .await?;
            decrement_room(&mut active_room);
        }
    }

    Ok(responses)
}

pub async fn force_daily_refresh(
    state: &AppState,
    user_id: &str,
    req: ForceDailyRefreshRequest,
) -> ApiResult<ForceDailyRefreshResponse> {
    let targets = force_refresh_targets(state, user_id, req).await?;
    let refreshed_cards = generate_fresh_daily_cards(state, user_id, &targets).await?;
    Ok(ForceDailyRefreshResponse { refreshed_cards })
}

pub async fn refresh_due_daily_topics(state: &AppState) -> ApiResult<u64> {
    let user_ids = users::list_ids(&state.db)
        .await
        .map_err(|err| err.into_status_body())?;
    let mut refreshed_cards = 0;

    for user_id in user_ids {
        let defaults = state
            .settings
            .get_settings()
            .map_err(|err| err.into_status_body())?;
        let settings = user_settings::get(&state.db, &user_id, defaults)
            .await
            .map_err(|err| err.into_status_body())?;
        let targets = topics::list_generated_targets(&state.db, &user_id)
            .await
            .map_err(|err| err.into_status_body())?;

        for (topic, tipcard_type) in targets {
            let topic_info = TopicInfo::from(topic.clone());
            let window_start = domain::scheduling::topic_daily_window_start(
                &topic_info,
                &settings.daily_time_zone,
                &settings.daily_update_time,
            );
            let window_start_key = window_start
                .naive_utc()
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();
            let last_window =
                daily_refresh::last_window_start(&state.db, &user_id, topic.id, &tipcard_type)
                    .await
                    .map_err(|err| err.into_status_body())?;
            if last_window.as_deref() == Some(window_start_key.as_str()) {
                continue;
            }

            let refreshed =
                generate_fresh_daily_cards(state, &user_id, &[(topic_info, topic.name.clone())])
                    .await?;
            daily_refresh::mark_window_refreshed(
                &state.db,
                &user_id,
                topic.id,
                &tipcard_type,
                window_start,
            )
            .await
            .map_err(|err| err.into_status_body())?;
            refreshed_cards += refreshed;
        }
    }

    Ok(refreshed_cards)
}

async fn force_refresh_targets(
    state: &AppState,
    user_id: &str,
    req: ForceDailyRefreshRequest,
) -> ApiResult<Vec<(TopicInfo, String)>> {
    let topic_names: Vec<String> = req
        .topics
        .split(',')
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
        .map(str::to_string)
        .collect();
    let requested_type = req
        .tipcard_type
        .unwrap_or_else(|| "repeatable_tip".to_string());
    let all_generated_topics = topic_names.is_empty() && requested_type.trim().is_empty();

    let targets = if all_generated_topics {
        topics::list_generated_targets(&state.db, user_id)
            .await
            .map_err(|err| err.into_status_body())?
            .into_iter()
            .map(|(topic, _)| {
                let name = topic.name.clone();
                (topic.into(), name)
            })
            .collect()
    } else {
        if !domain::tipcard::TipcardType::from_setting(&requested_type).is_generated() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Only generated daily cards can be force-refreshed".to_string(),
            ));
        }
        let mut targets = Vec::new();
        for topic_name in topic_names {
            let topic = get_or_create_topic(state, user_id, &topic_name, &requested_type).await?;
            targets.push((topic, topic_name));
        }
        targets
    };
    Ok(targets)
}

async fn generate_fresh_daily_cards(
    state: &AppState,
    user_id: &str,
    targets: &[(TopicInfo, String)],
) -> ApiResult<u64> {
    if targets.is_empty() {
        return Ok(0);
    }

    let defaults = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    let settings = user_settings::get(&state.db, user_id, defaults)
        .await
        .map_err(|err| err.into_status_body())?;
    let llm_reasoning = llm::ReasoningConfig::new(settings.llm_reasoning_effort.clone());
    let llm_compress_reasoning =
        llm::ReasoningConfig::new(settings.llm_compress_reasoning_effort.clone());
    let llm_compression_level =
        llm::CompressionLevel::from_setting(&settings.llm_compression_level);
    let mut active_room = active_card_room(state, user_id, settings.max_active_cards).await?;
    let mut responses = Vec::new();

    for (topic, topic_name) in targets {
        if matches!(active_room, Some(0)) {
            break;
        }
        generate_tipcard(
            state,
            user_id,
            topic_name,
            topic,
            &settings.prompt_template,
            &settings.llm_model,
            &settings.llm_api_key,
            &settings.llm_base_url,
            &llm_reasoning,
            &settings.llm_compress_model,
            &settings.llm_compress_base_url,
            llm_compression_level,
            &llm_compress_reasoning,
            &mut responses,
        )
        .await?;
        decrement_room(&mut active_room);
    }

    Ok(responses.len() as u64)
}

pub(crate) async fn create_custom_tipcard(
    state: &AppState,
    user_id: &str,
    req: pb::CustomTipcardRequest,
) -> ApiResult<TipCardJson> {
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
        fallback_title(&full_tip, "Custom card")
    } else {
        title.chars().take(96).collect::<String>()
    };

    let topic = get_or_create_topic(state, user_id, topic_name, "custom_tip").await?;
    let card_id = tipcards::create_custom(
        &state.db,
        user_id,
        topic.id,
        &title,
        &full_tip,
        &compressed_tip,
    )
    .await
    .map_err(|err| err.into_status_body())?;

    Ok(tip_response_json(
        card_id,
        topic_name,
        full_tip,
        compressed_tip,
        Vec::new(),
        "custom_tip".to_string(),
        false,
    ))
}

async fn generate_tipcard(
    state: &AppState,
    user_id: &str,
    topic_name: &str,
    topic: &TopicInfo,
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
) -> ApiResult<()> {
    let template = topic
        .prompt_template
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(template);
    let card_context =
        context::load_card_context(state, user_id, topic.id, &topic.tipcard_type).await?;
    let prompt = context::render_generation_prompt(topic_name, template, &card_context);
    let full_res =
        llm::generate_new_card(llm_model, &prompt, llm_api_key, llm_base_url, llm_reasoning).await;
    record_llm_token_usage(state, user_id, llm_model, "generate_card", &full_res.usage).await?;
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
        user_id,
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
        user_id,
        llm_compress_model,
        "generate_title",
        &title_res.usage,
    )
    .await?;
    let card_title = title_res.content;

    let card_id = tipcards::create_generated(
        &state.db,
        user_id,
        topic.id,
        &topic.tipcard_type,
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
        topic.tipcard_type.clone(),
        false,
    ));

    Ok(())
}

async fn create_manual_tipcard(
    state: &AppState,
    user_id: &str,
    topic_name: &str,
    topic: &TopicInfo,
    full_tip: String,
    compressed_tip: String,
    image_data: Vec<String>,
    responses: &mut Vec<TipCardJson>,
) -> ApiResult<()> {
    let title = fallback_title(&full_tip, "Manual card");
    let image_data_json = image_data_json(&image_data)?;
    let card_id = tipcards::create_manual(
        &state.db,
        user_id,
        topic.id,
        &topic.tipcard_type,
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
        topic.tipcard_type.clone(),
        false,
    ));

    Ok(())
}

async fn record_llm_token_usage(
    state: &AppState,
    user_id: &str,
    model: &str,
    purpose: &str,
    usage: &llm::TokenUsage,
) -> ApiResult<()> {
    token_usage::insert(&state.db, user_id, model, purpose, usage)
        .await
        .map_err(|err| err.into_status_body())
}

fn fallback_title(content: &str, fallback: &str) -> String {
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(fallback)
        .chars()
        .take(96)
        .collect::<String>()
}

fn decrement_room(active_room: &mut Option<usize>) {
    if let Some(room) = active_room.as_mut() {
        *room = room.saturating_sub(1);
    }
}

fn is_queue_tipcard(tipcard_type: &str) -> bool {
    domain::tipcard::is_queue_tipcard(tipcard_type)
}
