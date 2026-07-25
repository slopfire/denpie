use axum::http::StatusCode;

use crate::{
    AppState, context,
    db::repositories::{
        daily_refresh, documents, image_pool, tipcards, token_usage, topics, user_settings, users,
    },
    domain, image_store, llm,
    services::{
        tipcards::{active_card_room, image_data_json, parse_image_data, validate_image_data},
        topics::TopicService,
    },
    types::{
        ApiResult, ForceDailyRefreshRequest, ForceDailyRefreshResponse, TipCardJson,
        TipsJsonRequest,
    },
};

impl domain::scheduling::DailyWindowTopic for topics::TopicRecord {
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

#[derive(Clone, Copy, Debug, Default)]
pub struct TipService;

impl TipService {
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
        tipcards::stack_due_repeatable_cards(&state.db, user_id)
            .await
            .map_err(|err| err.into_status_body())?;
        let llm_reasoning = llm::ReasoningConfig::new(settings.llm_reasoning_effort.clone());
        let grounding_reasoning = llm::ReasoningConfig::new(settings.grounding_reasoning_effort());
        let llm_compression_level =
            llm::CompressionLevel::from_setting(&settings.llm_compression_level);
        let mut active_room = active_card_room(state, user_id, settings.max_active_cards).await?;

        for topic_name in topics_list.into_iter().take(count as usize) {
            let topic_name = topic_name.trim();
            if topic_name.is_empty() {
                continue;
            }

            let topic =
                TopicService::get_or_create_topic(state, user_id, topic_name, &requested_type)
                    .await
                    .map_err(|err| err.into_status_body())?;
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
                Self::create_manual_tipcard(
                    GenerationContext {
                        state,
                        user_id,
                        topic_name,
                        topic: &topic,
                        settings: &settings,
                    },
                    manual_content.clone(),
                    compact,
                    manual_image_data.clone(),
                    &mut responses,
                )
                .await?;
                decrement_room(&mut active_room);
                continue;
            }

            let daily_card_count = if is_queue_tipcard(&topic.tipcard_type)
                || topic.tipcard_type == "repeatable_tip"
            {
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
            let had_due_cards = !due_cards.is_empty();
            for card in due_cards {
                responses.push(tip_response_json(
                    card.id,
                    topic_name,
                    card.full_content,
                    card.compressed_content,
                    parse_image_data(&card.image_data),
                    topic.tipcard_type.clone(),
                    card.pinned,
                ));
            }
            if had_due_cards {
                let pending =
                    tipcards::count_pending(&state.db, user_id, topic.id, &topic.tipcard_type)
                        .await
                        .map_err(|err| err.into_status_body())?;
                if pending_needs_generation(pending) {
                    Self::generate_tipcard(
                        GenerationContext {
                            state,
                            user_id,
                            topic_name,
                            topic: &topic,
                            settings: &settings,
                        },
                        &settings.prompt_template,
                        GenerationLlmConfig {
                            model: &settings.llm_model,
                            grounding_model: settings.grounding_model(),
                            api_key: &settings.llm_api_key,
                            base_url: &settings.llm_base_url,
                            reasoning: &llm_reasoning,
                            grounding_reasoning: &grounding_reasoning,
                            compression_level: llm_compression_level,
                        },
                        "pending",
                        count as usize,
                        false,
                        &mut responses,
                    )
                    .await?;
                }
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
                let daily_count = daily_cards.len();
                for card in daily_cards {
                    responses.push(tip_response_json(
                        card.id,
                        topic_name,
                        card.full_content,
                        card.compressed_content,
                        parse_image_data(&card.image_data),
                        topic.tipcard_type.clone(),
                        card.pinned,
                    ));
                }
                let remaining_daily_cards = daily_card_count.saturating_sub(daily_count);
                let cards_to_generate = active_room.map_or(remaining_daily_cards, |room| {
                    remaining_daily_cards.min(room)
                });
                if cards_to_generate > 0 {
                    daily_refresh::mark_window_refreshed(
                        &state.db,
                        user_id,
                        topic.id,
                        &topic.tipcard_type,
                        daily_window_start,
                    )
                    .await
                    .map_err(|err| err.into_status_body())?;
                }
                let cards_to_load = if cards_to_generate == 0 {
                    0
                } else if topic.tipcard_type == "repeatable_tip" {
                    count.min(10) as usize
                } else {
                    cards_to_generate
                };
                if cards_to_load > 0 {
                    let response_count = responses.len();
                    let result = Self::generate_tipcard(
                        GenerationContext {
                            state,
                            user_id,
                            topic_name,
                            topic: &topic,
                            settings: &settings,
                        },
                        &settings.prompt_template,
                        GenerationLlmConfig {
                            model: &settings.llm_model,
                            grounding_model: settings.grounding_model(),
                            api_key: &settings.llm_api_key,
                            base_url: &settings.llm_base_url,
                            reasoning: &llm_reasoning,
                            grounding_reasoning: &grounding_reasoning,
                            compression_level: llm_compression_level,
                        },
                        "active",
                        cards_to_load,
                        true,
                        &mut responses,
                    )
                    .await;
                    match result {
                        Ok(_) => {}
                        Err(err) => {
                            let _ = daily_refresh::clear_window_refreshed(
                                &state.db,
                                user_id,
                                topic.id,
                                &topic.tipcard_type,
                                daily_window_start,
                            )
                            .await;
                            return Err(err);
                        }
                    }
                    if responses.len() > response_count {
                        decrement_room(&mut active_room);
                    }
                }
            } else if active_room.is_none_or(|room| room > 0) {
                let daily_window_start = domain::scheduling::topic_daily_window_start(
                    &topic,
                    &settings.daily_time_zone,
                    &settings.daily_update_time,
                );
                daily_refresh::mark_window_refreshed(
                    &state.db,
                    user_id,
                    topic.id,
                    &topic.tipcard_type,
                    daily_window_start,
                )
                .await
                .map_err(|err| err.into_status_body())?;
                if let Err(err) = Self::generate_tipcard(
                    GenerationContext {
                        state,
                        user_id,
                        topic_name,
                        topic: &topic,
                        settings: &settings,
                    },
                    &settings.prompt_template,
                    GenerationLlmConfig {
                        model: &settings.llm_model,
                        grounding_model: settings.grounding_model(),
                        api_key: &settings.llm_api_key,
                        base_url: &settings.llm_base_url,
                        reasoning: &llm_reasoning,
                        grounding_reasoning: &grounding_reasoning,
                        compression_level: llm_compression_level,
                    },
                    "active",
                    1,
                    true,
                    &mut responses,
                )
                .await
                {
                    let _ = daily_refresh::clear_window_refreshed(
                        &state.db,
                        user_id,
                        topic.id,
                        &topic.tipcard_type,
                        daily_window_start,
                    )
                    .await;
                    return Err(err);
                }
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
        let targets = Self::force_refresh_targets(state, user_id, req).await?;
        let refreshed_cards = Self::generate_fresh_daily_cards(state, user_id, &targets).await?;
        if refreshed_cards > 0 {
            Self::mark_targets_current_window(
                state,
                user_id,
                &targets[..(refreshed_cards as usize).min(targets.len())],
            )
            .await?;
        }
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
                let window_start = domain::scheduling::topic_daily_window_start(
                    &topic,
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

                daily_refresh::mark_window_refreshed(
                    &state.db,
                    &user_id,
                    topic.id,
                    &tipcard_type,
                    window_start,
                )
                .await
                .map_err(|err| err.into_status_body())?;
                let refreshed = match Self::generate_fresh_daily_cards(
                    state,
                    &user_id,
                    &[(topic.clone(), topic.name.clone())],
                )
                .await
                {
                    Ok(refreshed) => refreshed,
                    Err(err) => {
                        let _ = daily_refresh::clear_window_refreshed(
                            &state.db,
                            &user_id,
                            topic.id,
                            &tipcard_type,
                            window_start,
                        )
                        .await;
                        return Err(err);
                    }
                };
                refreshed_cards += refreshed;
            }
        }

        Ok(refreshed_cards)
    }

    async fn force_refresh_targets(
        state: &AppState,
        user_id: &str,
        req: ForceDailyRefreshRequest,
    ) -> ApiResult<Vec<(topics::TopicRecord, String)>> {
        let topic_names: Vec<String> = req
            .topics
            .split(',')
            .map(str::trim)
            .filter(|topic| !topic.is_empty())
            .map(str::to_string)
            .collect();
        let requested_type = req.tipcard_type.unwrap_or_default();
        let all_generated_topics = topic_names.is_empty() && requested_type.trim().is_empty();

        let targets = if all_generated_topics {
            topics::list_generated_targets(&state.db, user_id)
                .await
                .map_err(|err| err.into_status_body())?
        } else {
            if !domain::tipcard::TipcardType::from_setting(&requested_type).is_generated() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Only generated daily cards can be force-refreshed".to_string(),
                ));
            }
            let mut targets = Vec::new();
            for topic_name in topic_names {
                let topic =
                    TopicService::get_or_create_topic(state, user_id, &topic_name, &requested_type)
                        .await
                        .map_err(|err| err.into_status_body())?;
                targets.push((topic, topic_name));
            }
            targets
        };
        Ok(targets)
    }

    async fn mark_targets_current_window(
        state: &AppState,
        user_id: &str,
        targets: &[(topics::TopicRecord, String)],
    ) -> ApiResult<()> {
        let defaults = state
            .settings
            .get_settings()
            .map_err(|err| err.into_status_body())?;
        let settings = user_settings::get(&state.db, user_id, defaults)
            .await
            .map_err(|err| err.into_status_body())?;

        for (topic, _) in targets {
            let window_start = domain::scheduling::topic_daily_window_start(
                topic,
                &settings.daily_time_zone,
                &settings.daily_update_time,
            );
            daily_refresh::mark_window_refreshed(
                &state.db,
                user_id,
                topic.id,
                &topic.tipcard_type,
                window_start,
            )
            .await
            .map_err(|err| err.into_status_body())?;
        }
        Ok(())
    }

    async fn generate_fresh_daily_cards(
        state: &AppState,
        user_id: &str,
        targets: &[(topics::TopicRecord, String)],
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
        let grounding_reasoning = llm::ReasoningConfig::new(settings.grounding_reasoning_effort());
        let llm_compression_level =
            llm::CompressionLevel::from_setting(&settings.llm_compression_level);
        tipcards::stack_due_repeatable_cards(&state.db, user_id)
            .await
            .map_err(|err| err.into_status_body())?;
        let mut active_room = active_card_room(state, user_id, settings.max_active_cards).await?;
        let mut responses = Vec::new();
        let mut created_total = 0_u64;

        for (topic, topic_name) in targets {
            if topic.tipcard_type == "repeatable_tip" {
                tipcards::park_unseen_active_topic_cards(&state.db, user_id, topic.id)
                    .await
                    .map_err(|err| err.into_status_body())?;
            }
            let primary_status = if topic.tipcard_type == "repeatable_tip"
                && tipcards::has_active_topic_card(&state.db, user_id, topic.id)
                    .await
                    .map_err(|err| err.into_status_body())?
            {
                "pending"
            } else {
                "active"
            };
            if primary_status == "active" && matches!(active_room, Some(0)) {
                break;
            }
            let created = Self::generate_tipcard(
                GenerationContext {
                    state,
                    user_id,
                    topic_name,
                    topic,
                    settings: &settings,
                },
                &settings.prompt_template,
                GenerationLlmConfig {
                    model: &settings.llm_model,
                    grounding_model: settings.grounding_model(),
                    api_key: &settings.llm_api_key,
                    base_url: &settings.llm_base_url,
                    reasoning: &llm_reasoning,
                    grounding_reasoning: &grounding_reasoning,
                    compression_level: llm_compression_level,
                },
                primary_status,
                1,
                false,
                &mut responses,
            )
            .await?;
            created_total += u64::from(created > 0);
            if primary_status == "active" {
                decrement_room(&mut active_room);
            }
        }

        Ok(created_total)
    }

    pub async fn create_custom_tipcard(
        state: &AppState,
        user_id: &str,
        data: CustomTipcardData,
    ) -> ApiResult<TipCardJson> {
        let topic_name = data.topic.trim();
        if topic_name.is_empty() {
            return Err((StatusCode::BAD_REQUEST, "topic is required".to_string()));
        }

        let full_tip = data.full_content.trim().to_string();
        if full_tip.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "full_content is required".to_string(),
            ));
        }

        let compressed_tip = data.compressed_content.trim().to_string();
        let compressed_tip = if compressed_tip.is_empty() {
            full_tip.clone()
        } else {
            compressed_tip
        };
        let title = data.title.trim().to_string();
        let title = if title.is_empty() {
            fallback_title(&full_tip, "Custom card")
        } else {
            title.chars().take(96).collect::<String>()
        };

        let topic = TopicService::get_or_create_topic(state, user_id, topic_name, "custom_tip")
            .await
            .map_err(|err| err.into_status_body())?;
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

    pub async fn create_manual_tipcard(
        ctx: GenerationContext<'_>,
        full_tip: String,
        compressed_tip: String,
        image_data: Vec<String>,
        responses: &mut Vec<TipCardJson>,
    ) -> ApiResult<()> {
        let title = fallback_title(&full_tip, "Manual card");
        let image_data_json = image_data_json(&image_data)?;
        let card_id = tipcards::create_manual(
            &ctx.state.db,
            tipcards::CreateManualParams {
                user_id: ctx.user_id,
                topic_id: ctx.topic.id,
                tipcard_type: &ctx.topic.tipcard_type,
                title: &title,
                full_content: &full_tip,
                compressed_content: &compressed_tip,
                image_data_json: &image_data_json,
            },
        )
        .await
        .map_err(|err| err.into_status_body())?;
        image_store::replace_card_images(
            &ctx.state.db,
            &ctx.state.image_dir,
            ctx.user_id,
            card_id,
            image_data.clone(),
        )
        .await?;

        responses.push(tip_response_json(
            card_id,
            ctx.topic_name,
            full_tip,
            compressed_tip,
            image_data,
            ctx.topic.tipcard_type.clone(),
            false,
        ));
        Ok(())
    }
}

pub struct CustomTipcardData {
    pub topic: String,
    pub full_content: String,
    pub compressed_content: String,
    pub title: String,
}

pub(crate) struct GenerationLlmConfig<'a> {
    pub(crate) model: &'a str,
    pub(crate) grounding_model: &'a str,
    pub(crate) api_key: &'a str,
    pub(crate) base_url: &'a str,
    pub(crate) reasoning: &'a llm::ReasoningConfig,
    pub(crate) grounding_reasoning: &'a llm::ReasoningConfig,
    pub(crate) compression_level: llm::CompressionLevel,
}

pub(crate) struct GenerationContext<'a> {
    pub(crate) state: &'a AppState,
    pub(crate) user_id: &'a str,
    pub(crate) topic_name: &'a str,
    pub(crate) topic: &'a topics::TopicRecord,
    pub(crate) settings: &'a crate::config::Settings,
}

impl TipService {
    pub async fn generate_tipcard(
        ctx: GenerationContext<'_>,
        template: &str,
        llm: GenerationLlmConfig<'_>,
        primary_status: &str,
        batch_size: usize,
        promote_pending: bool,
        responses: &mut Vec<TipCardJson>,
    ) -> ApiResult<usize> {
        let grounding = domain::grounding::GroundingStrategy::from_setting(
            ctx.topic
                .grounding_strategy
                .as_deref()
                .unwrap_or(&ctx.settings.grounding_strategy),
        );
        let image_strategy = domain::grounding::ImageStrategy::from_setting(
            ctx.topic
                .image_strategy
                .as_deref()
                .unwrap_or(&ctx.settings.image_strategy),
        );
        let (generation_model, generation_reasoning) =
            if matches!(grounding, domain::grounding::GroundingStrategy::Factual) {
                (llm.model, llm.reasoning)
            } else {
                (llm.grounding_model, llm.grounding_reasoning)
            };

        let should_promote = promote_pending && primary_status == "active";
        let pending_count = tipcards::count_pending(
            &ctx.state.db,
            ctx.user_id,
            ctx.topic.id,
            &ctx.topic.tipcard_type,
        )
        .await
        .map_err(|err| err.into_status_body())?;

        // Above the low-water mark, delivery is a DB-only queue promotion.
        if should_promote && !pending_needs_generation(pending_count) {
            if Self::serve_pending_card(&ctx, &llm, image_strategy, responses).await? {
                return Ok(batch_size.max(1));
            }
        }
        if !pending_needs_generation(pending_count) {
            return Ok(0);
        }

        let template = ctx
            .topic
            .prompt_template
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(template);
        let card_context = context::load_card_context(
            ctx.state,
            ctx.user_id,
            ctx.topic.id,
            &ctx.topic.tipcard_type,
        )
        .await?;
        let prompt = context::render_generation_prompt(ctx.topic_name, template, &card_context);
        let compression_level = ctx
            .topic
            .compression_level
            .as_deref()
            .map(llm::CompressionLevel::from_setting)
            .unwrap_or(llm.compression_level);

        // For RAG, retrieve top-k chunks explicitly assigned to this topic.
        let documents: Vec<llm::DocChunk> =
            if matches!(grounding, domain::grounding::GroundingStrategy::Rag) {
                documents::retrieve_chunks(
                    &ctx.state.db,
                    ctx.user_id,
                    ctx.topic.id,
                    ctx.topic_name,
                    12,
                )
                .await
                .map_err(|err| err.into_status_body())?
                .into_iter()
                .map(|chunk| llm::DocChunk { chunk })
                .collect()
            } else {
                Vec::new()
            };

        let outcome = llm::ground_and_generate(
            grounding,
            llm::GroundingInput {
                topic_name: ctx.topic_name,
                rendered_prompt: &prompt,
                compression_level,
                model: generation_model,
                api_key: llm.api_key,
                api_base: llm.base_url,
                reasoning: generation_reasoning,
                existing_titles: card_context.existing_titles(),
                documents: &documents,
                daily_card_count: batch_size.clamp(5, 10) as i64,
                search: llm::SearchConfig {
                    external_key: &ctx.settings.search_api_key,
                    base_url: &ctx.settings.search_base_url,
                },
            },
        )
        .await;

        Self::record_llm_token_usage(
            ctx.state,
            ctx.user_id,
            generation_model,
            "generate_card",
            &outcome.usage,
        )
        .await?;

        if !outcome.citations.is_empty() {
            tracing::info!(
                topic = ctx.topic_name,
                citations = ?outcome.citations,
                "grounded card citations"
            );
        }

        // Another request may have refilled this topic during the LLM call.
        let current_pending = tipcards::count_pending(
            &ctx.state.db,
            ctx.user_id,
            ctx.topic.id,
            &ctx.topic.tipcard_type,
        )
        .await
        .map_err(|err| err.into_status_body())?;
        if !pending_needs_generation(current_pending) {
            if should_promote {
                Self::serve_pending_card(&ctx, &llm, image_strategy, responses).await?;
            }
            return Ok(0);
        }

        // Every grounding strategy writes its complete batch to pending first.
        let mut created_count = 0;
        for card in outcome.cards() {
            if let Err(err) = tipcards::create_generated_with_status(
                &ctx.state.db,
                ctx.user_id,
                ctx.topic.id,
                &ctx.topic.tipcard_type,
                &card.title,
                &card.full_content,
                &card.compressed_content,
                card.use_image,
                &card.image_query,
                "pending",
            )
            .await
            {
                tracing::warn!(error = ?err, "failed to persist pending card");
            } else {
                created_count += 1;
            }
        }

        if !should_promote {
            return Ok(created_count);
        }

        Self::serve_pending_card(&ctx, &llm, image_strategy, responses).await?;
        Ok(created_count)
    }

    async fn serve_pending_card(
        ctx: &GenerationContext<'_>,
        llm: &GenerationLlmConfig<'_>,
        image_strategy: domain::grounding::ImageStrategy,
        responses: &mut Vec<TipCardJson>,
    ) -> ApiResult<bool> {
        let Some(card) = tipcards::take_pending_card(
            &ctx.state.db,
            ctx.user_id,
            ctx.topic.id,
            &ctx.topic.tipcard_type,
        )
        .await
        .map_err(|err| err.into_status_body())?
        else {
            return Ok(false);
        };

        let image_data = Self::retrieve_card_image(
            ctx,
            llm,
            image_strategy,
            card.id,
            &card.title,
            &card.full_content,
            card.use_image,
            &card.image_query,
        )
        .await;
        responses.push(tip_response_json(
            card.id,
            ctx.topic_name,
            card.full_content,
            card.compressed_content,
            if image_data.is_empty() {
                parse_image_data(&card.image_data)
            } else {
                image_data
            },
            ctx.topic.tipcard_type.clone(),
            card.pinned,
        ));
        Ok(true)
    }

    /// Retrieve and persist a single illustration for a freshly generated card.
    /// Returns the data-URLs stored (empty on no-image or any failure).
    #[allow(clippy::too_many_arguments)]
    async fn retrieve_card_image(
        ctx: &GenerationContext<'_>,
        llm: &GenerationLlmConfig<'_>,
        strategy: domain::grounding::ImageStrategy,
        card_id: i64,
        card_title: &str,
        card_content: &str,
        use_image: bool,
        image_query: &str,
    ) -> Vec<String> {
        if !use_image
            || image_query.trim().is_empty()
            || matches!(strategy, domain::grounding::ImageStrategy::None)
        {
            return Vec::new();
        }

        let pool = match image_pool::list_pool_images(&ctx.state.db, ctx.user_id).await {
            Ok(rows) => rows,
            Err(err) => {
                tracing::warn!(error = ?err, "failed to list pool images");
                Vec::new()
            }
        };
        let pool_meta: Vec<llm::PoolImageMeta> = pool
            .iter()
            .map(|row| llm::PoolImageMeta {
                id: row.id,
                name: row.name.clone(),
                description: row.description.clone(),
            })
            .collect();
        let image_sources =
            domain::grounding::image_sources_from_setting(&ctx.settings.image_sources);

        let (image_model, image_reasoning) =
            if matches!(strategy, domain::grounding::ImageStrategy::Agentic) {
                (llm.grounding_model, llm.grounding_reasoning)
            } else {
                (llm.model, llm.reasoning)
            };

        let retrieved = llm::retrieve_image(
            strategy,
            llm::ImageInput {
                topic_name: ctx.topic_name,
                card_title,
                card_content,
                image_query,
                model: image_model,
                api_key: llm.api_key,
                api_base: llm.base_url,
                reasoning: image_reasoning,
                pool: &pool_meta,
                sources: &image_sources,
                search_api_key: &ctx.settings.search_api_key,
                search_base_url: &ctx.settings.search_base_url,
            },
        )
        .await;

        let Some(image) = retrieved else {
            return Vec::new();
        };

        // Resolve the data-URL: pool strategy returns a pool_id whose bytes live on disk.
        let data_url = match image.pool_id {
            Some(pool_id) => match pool.iter().find(|row| row.id == pool_id) {
                Some(row) => match Self::pool_image_data_url(ctx, row).await {
                    Some(data_url) => data_url,
                    None => return Vec::new(),
                },
                None => return Vec::new(),
            },
            None => image.data_url,
        };

        match image_store::replace_card_images(
            &ctx.state.db,
            &ctx.state.image_dir,
            ctx.user_id,
            card_id,
            vec![data_url.clone()],
        )
        .await
        {
            Ok(()) => vec![data_url],
            Err((_, message)) => {
                tracing::warn!(message, "failed to store retrieved card image");
                Vec::new()
            }
        }
    }

    /// Load a pool image's bytes from disk and encode to a base64 data-URL.
    async fn pool_image_data_url(
        ctx: &GenerationContext<'_>,
        row: &image_pool::ImagePoolRecord,
    ) -> Option<String> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let path = ctx.state.image_dir.join(&row.storage_path);
        match tokio::fs::read(&path).await {
            Ok(bytes) => Some(format!(
                "data:{};base64,{}",
                row.mime_type,
                STANDARD.encode(&bytes)
            )),
            Err(err) => {
                tracing::warn!(error = ?err, path = ?path, "failed to read pool image bytes");
                None
            }
        }
    }

    pub async fn record_llm_token_usage(
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

    pub async fn aggregate_token_spend(
        state: &AppState,
        user_id: &str,
    ) -> crate::error::AppResult<token_usage::TokenSpendRecord> {
        token_usage::aggregate_spend(&state.db, user_id).await
    }
}

pub(crate) fn tip_response_json(
    id: i64,
    topic: &str,
    full_content: String,
    compressed_content: String,
    image_data: Vec<String>,
    tipcard_type: String,
    pinned: bool,
) -> TipCardJson {
    TipCardJson {
        id,
        topic: topic.to_string(),
        full_content,
        compressed_content,
        image_data,
        tipcard_type,
        pinned,
    }
}

pub(crate) fn fallback_title(content: &str, fallback: &str) -> String {
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(fallback)
        .chars()
        .take(96)
        .collect::<String>()
}

pub(crate) fn decrement_room(active_room: &mut Option<usize>) {
    if let Some(room) = active_room.as_mut() {
        *room = room.saturating_sub(1);
    }
}

pub(crate) fn is_queue_tipcard(tipcard_type: &str) -> bool {
    domain::tipcard::is_queue_tipcard(tipcard_type)
}

fn pending_needs_generation(pending_count: i64) -> bool {
    pending_count <= 2
}

#[cfg(test)]
mod generation_queue_tests {
    use super::pending_needs_generation;

    #[test]
    fn bootstraps_empty_queue_and_refills_only_at_low_water_mark() {
        assert!(pending_needs_generation(0));
        assert!(pending_needs_generation(1));
        assert!(pending_needs_generation(2));
        assert!(!pending_needs_generation(3));
        assert!(!pending_needs_generation(10));
    }
}
