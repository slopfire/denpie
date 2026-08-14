use axum::http::StatusCode;

use crate::{
    AppState, context,
    db::repositories::{
        daily_refresh, documents, tipcards, token_usage, topics, user_settings, users,
    },
    domain, image_store, llm,
    services::{
        tipcards::{active_card_room, image_data_json, parse_image_data, validate_image_data},
        topics::TopicService,
    },
    types::{
        ApiResult, ContinueDailyReviewRequest, ForceDailyRefreshRequest, ForceDailyRefreshResponse,
        TipCardJson, TipsJsonRequest,
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
    pub async fn review_and_advance(
        state: &AppState,
        user_id: &str,
        card_id: i64,
        grade: u8,
        action: &str,
    ) -> ApiResult<crate::services::review::ReviewAdvanceResult> {
        let topic = topics::find_for_card(&state.db, user_id, card_id)
            .await
            .map_err(|err| err.into_status_body())?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Card not found".to_string()))?;
        let defaults = state
            .settings
            .get_settings()
            .map_err(|err| err.into_status_body())?;
        let settings = user_settings::get(&state.db, user_id, defaults)
            .await
            .map_err(|err| err.into_status_body())?;
        let window_start = domain::scheduling::topic_daily_window_start(
            &topic,
            &settings.daily_time_zone,
            &settings.daily_update_time,
        );
        let base_limit = domain::scheduling::topic_daily_card_count(&topic) as i64;
        let extra = if topic.tipcard_type == "repeatable_tip" {
            tipcards::extra_cards_in_window(
                &state.db,
                user_id,
                topic.id,
                &topic.tipcard_type,
                window_start,
            )
            .await
            .map_err(|err| err.into_status_body())? as i64
        } else {
            0
        };
        let mut result = state
            .reviews
            .apply_review_and_advance(
                user_id,
                card_id,
                grade,
                action,
                crate::services::review::ReviewAdvancePolicy {
                    window_start,
                    daily_limit: base_limit.saturating_add(extra),
                },
            )
            .await
            .map_err(|err| err.into_status_body())?;

        if result.tipcard_type == "repeatable_tip" && pending_needs_generation(result.pending_count)
        {
            if let Some(state) = state.self_arc.get() {
                Self::spawn_pending_refill(
                    state.clone(),
                    user_id.to_string(),
                    result.topic_id,
                    result.topic_name.clone(),
                    result.tipcard_type.clone(),
                );
                result.refill_scheduled = true;
            }
        }
        Ok(result)
    }

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
                    // Due cards are already in the response; a refill failure must
                    // not discard them. Log and let the next request retry.
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
                        "pending",
                        count as usize,
                        false,
                        &mut responses,
                    )
                    .await
                    {
                        tracing::error!(
                            topic = topic_name,
                            error = %err.1,
                            "pending refill failed; serving due cards anyway"
                        );
                    }
                }
                continue;
            }

            if !is_queue_tipcard(&topic.tipcard_type) {
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
                let reviewed_count = if topic.tipcard_type == "repeatable_tip" {
                    tipcards::count_reviewed_in_window(
                        &state.db,
                        user_id,
                        topic.id,
                        daily_window_start,
                    )
                    .await
                    .map_err(|err| err.into_status_body())?
                } else {
                    0
                };
                let daily_review_limit = if topic.tipcard_type == "repeatable_tip" {
                    let extra_cards = tipcards::extra_cards_in_window(
                        &state.db,
                        user_id,
                        topic.id,
                        &topic.tipcard_type,
                        daily_window_start,
                    )
                    .await
                    .map_err(|err| err.into_status_body())?;
                    daily_card_count.saturating_add(extra_cards)
                } else {
                    daily_card_count
                };
                let remaining_daily_cards = if topic.tipcard_type == "repeatable_tip" {
                    daily_review_limit.saturating_sub(reviewed_count)
                } else {
                    daily_card_count.saturating_sub(daily_count)
                };
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
                let mut needs_review_replacement = false;
                if topic.tipcard_type == "repeatable_tip"
                    && daily_count == 0
                    && remaining_daily_cards > 0
                {
                    // An excluded card can still be due when callers are merely
                    // paginating. Only promote its replacement after review has
                    // actually moved it off the due queue.
                    let excluded_card_is_still_due = !exclude_card_ids.is_empty()
                        && !tipcards::find_due_topic_cards(
                            &state.db,
                            user_id,
                            topic.id,
                            &topic.tipcard_type,
                            &[],
                            1,
                        )
                        .await
                        .map_err(|err| err.into_status_body())?
                        .is_empty();
                    if !excluded_card_is_still_due {
                        // The dashboard excludes the card it just reviewed. That card
                        // remains scheduled as active, so generating its replacement
                        // must not be blocked by the user's active-card limit.
                        needs_review_replacement = !exclude_card_ids.is_empty();
                        let image_strategy = domain::grounding::ImageStrategy::from_setting(
                            topic
                                .image_strategy
                                .as_deref()
                                .unwrap_or(&settings.image_strategy),
                        );
                        if Self::serve_pending_card(
                            &GenerationContext {
                                state,
                                user_id,
                                topic_name,
                                topic: &topic,
                                settings: &settings,
                            },
                            &GenerationLlmConfig {
                                model: &settings.llm_model,
                                grounding_model: settings.grounding_model(),
                                api_key: &settings.llm_api_key,
                                base_url: &settings.llm_base_url,
                                reasoning: &llm_reasoning,
                                grounding_reasoning: &grounding_reasoning,
                                compression_level: llm_compression_level,
                            },
                            image_strategy,
                            false,
                            &mut responses,
                        )
                        .await?
                        {
                            // Queue delivery replaces the reviewed card. It must
                            // not be blocked by the generation-only capacity gate.
                            Self::maybe_spawn_pending_refill(&GenerationContext {
                                state,
                                user_id,
                                topic_name,
                                topic: &topic,
                                settings: &settings,
                            })
                            .await;
                            decrement_room(&mut active_room);
                            continue;
                        }
                    }
                }
                let cards_to_generate = if topic.tipcard_type == "repeatable_tip" && daily_count > 0
                {
                    // Repeatable topics advance one reviewed card at a time. An
                    // unreviewed card already in the daily flow must be returned
                    // on its own rather than accompanied by the next queued card.
                    0
                } else if needs_review_replacement {
                    remaining_daily_cards
                } else {
                    active_room.map_or(remaining_daily_cards, |room| {
                        remaining_daily_cards.min(room)
                    })
                };
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
                    REPEATABLE_BATCH
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
                            if responses.len() == response_count {
                                // Nothing was served: surface the failure so the
                                // frontend can toast the reason.
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
                            // The daily set was already served; the refill can
                            // retry on the next request.
                            tracing::error!(
                                topic = topic_name,
                                error = %err.1,
                                "generation failed after daily cards were served"
                            );
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
        let rotate_repeatable =
            !req.topics.trim().is_empty() && req.tipcard_type.as_deref() == Some("repeatable_tip");
        let targets = Self::force_refresh_targets(state, user_id, req).await?;
        let refreshed_cards =
            Self::generate_fresh_daily_cards(state, user_id, &targets, rotate_repeatable).await?;
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

    /// Start one more full daily set for a repeatable topic in the current
    /// daily window. The immediate card is rotated into view now; subsequent
    /// cards are delivered normally as the learner reviews this additional set.
    pub async fn continue_daily_review(
        state: &AppState,
        user_id: &str,
        req: ContinueDailyReviewRequest,
    ) -> ApiResult<ForceDailyRefreshResponse> {
        let topic_names = req
            .topics
            .split(',')
            .map(str::trim)
            .filter(|topic| !topic.is_empty())
            .count();
        if topic_names != 1 || req.tipcard_type.as_deref() != Some("repeatable_tip") {
            return Err((
                StatusCode::BAD_REQUEST,
                "Continue is only available for one repeatable topic".to_string(),
            ));
        }

        let targets = Self::force_refresh_targets(
            state,
            user_id,
            ForceDailyRefreshRequest {
                topics: req.topics,
                tipcard_type: req.tipcard_type,
            },
        )
        .await?;
        if targets.len() != 1 || targets[0].0.tipcard_type != "repeatable_tip" {
            return Err((
                StatusCode::BAD_REQUEST,
                "Continue is only available for one repeatable topic".to_string(),
            ));
        }

        let refreshed_cards =
            Self::generate_fresh_daily_cards(state, user_id, &targets, true).await?;
        if refreshed_cards == 0 {
            return Err((
                StatusCode::CONFLICT,
                "Could not prepare another card to continue this review".to_string(),
            ));
        }

        let defaults = state
            .settings
            .get_settings()
            .map_err(|err| err.into_status_body())?;
        let settings = user_settings::get(&state.db, user_id, defaults)
            .await
            .map_err(|err| err.into_status_body())?;
        let topic = &targets[0].0;
        let daily_window_start = domain::scheduling::topic_daily_window_start(
            topic,
            &settings.daily_time_zone,
            &settings.daily_update_time,
        );
        let extra_cards = domain::scheduling::topic_daily_card_count(topic);
        tipcards::add_extra_cards(
            &state.db,
            user_id,
            topic.id,
            &topic.tipcard_type,
            daily_window_start,
            extra_cards,
        )
        .await
        .map_err(|err| err.into_status_body())?;
        Self::mark_targets_current_window(state, user_id, &targets).await?;

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
                let last_window =
                    daily_refresh::last_window_start(&state.db, &user_id, topic.id, &tipcard_type)
                        .await
                        .map_err(|err| err.into_status_body())?;
                if last_window == Some(window_start) {
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
                    false,
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
        rotate_repeatable: bool,
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
            if rotate_repeatable && topic.tipcard_type == "repeatable_tip" {
                let image_strategy = domain::grounding::ImageStrategy::from_setting(
                    topic
                        .image_strategy
                        .as_deref()
                        .unwrap_or(&settings.image_strategy),
                );
                if Self::serve_pending_card(
                    &GenerationContext {
                        state,
                        user_id,
                        topic_name,
                        topic,
                        settings: &settings,
                    },
                    &GenerationLlmConfig {
                        model: &settings.llm_model,
                        grounding_model: settings.grounding_model(),
                        api_key: &settings.llm_api_key,
                        base_url: &settings.llm_base_url,
                        reasoning: &llm_reasoning,
                        grounding_reasoning: &grounding_reasoning,
                        compression_level: llm_compression_level,
                    },
                    image_strategy,
                    true,
                    &mut responses,
                )
                .await?
                {
                    created_total += 1;
                    continue;
                }
            }
            if topic.tipcard_type == "repeatable_tip" {
                tipcards::park_unseen_active_topic_cards(&state.db, user_id, topic.id)
                    .await
                    .map_err(|err| err.into_status_body())?;
            }
            let primary_status = if topic.tipcard_type == "repeatable_tip"
                && !rotate_repeatable
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
            let batch = if topic.tipcard_type == "repeatable_tip" {
                REPEATABLE_BATCH
            } else {
                1
            };
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
                batch,
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

        // Delivery must never wait for an LLM refill when the next queued card is
        // already available. Refill in the background once the queue approaches
        // empty so later reviews find cards ready.
        if should_promote && pending_count > 0 {
            if Self::serve_pending_card(&ctx, &llm, image_strategy, false, responses).await? {
                Self::maybe_spawn_pending_refill(&ctx).await;
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

        let outcome = match llm::ground_and_generate(
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
                daily_card_count: batch_size.clamp(5, 12) as i64,
                search: llm::SearchConfig {
                    provider: &ctx.settings.search_provider,
                    external_key: &ctx.settings.search_api_key,
                    base_url: &ctx.settings.search_base_url,
                },
            },
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(message) => {
                tracing::error!(
                    topic = ctx.topic_name,
                    strategy = ?grounding,
                    error = %message,
                    "card generation failed"
                );
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Card generation failed: {message}"),
                ));
            }
        };

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
                Self::serve_pending_card(&ctx, &llm, image_strategy, false, responses).await?;
            }
            return Ok(0);
        }

        // Every grounding strategy writes its complete batch to pending first.
        // The repository locks the topic and rechecks queue depth, preventing
        // concurrent LLM requests from persisting duplicate batches.
        let cards = outcome
            .cards()
            .map(|card| tipcards::GeneratedCardParams {
                title: &card.title,
                full_content: &card.full_content,
                compressed_content: &card.compressed_content,
                use_image: card.use_image,
                image_query: &card.image_query,
            })
            .collect::<Vec<_>>();
        let created_count = tipcards::create_pending_batch_if_needed(
            &ctx.state.db,
            ctx.user_id,
            ctx.topic.id,
            &ctx.topic.tipcard_type,
            REFILL_LOW_WATER,
            &cards,
        )
        .await
        .map_err(|err| err.into_status_body())?
        .len();

        if !should_promote {
            return Ok(created_count);
        }

        Self::serve_pending_card(&ctx, &llm, image_strategy, false, responses).await?;
        Ok(created_count)
    }

    async fn serve_pending_card(
        ctx: &GenerationContext<'_>,
        _llm: &GenerationLlmConfig<'_>,
        _image_strategy: domain::grounding::ImageStrategy,
        replace_unseen: bool,
        responses: &mut Vec<TipCardJson>,
    ) -> ApiResult<bool> {
        // Skip (and delete) cards whose content marks a failed generation, so a
        // "Failed parsing text" or "LLM Error" card is never shown to the user.
        for _ in 0..8 {
            let card = if replace_unseen {
                tipcards::replace_unseen_with_pending_card(
                    &ctx.state.db,
                    ctx.user_id,
                    ctx.topic.id,
                    &ctx.topic.tipcard_type,
                )
                .await
            } else {
                tipcards::take_pending_card(
                    &ctx.state.db,
                    ctx.user_id,
                    ctx.topic.id,
                    &ctx.topic.tipcard_type,
                )
                .await
            }
            .map_err(|err| err.into_status_body())?;
            let Some(card) = card else {
                return Ok(false);
            };

            let failed = domain::tipcard::is_failed_generation_content(&card.full_content)
                || domain::tipcard::is_failed_generation_content(&card.compressed_content)
                || domain::tipcard::is_failed_generation_content(&card.title);
            if !failed {
                responses.push(tip_response_json(
                    card.id,
                    ctx.topic_name,
                    card.full_content,
                    card.compressed_content,
                    parse_image_data(&card.image_data),
                    ctx.topic.tipcard_type.clone(),
                    card.pinned,
                ));
                return Ok(true);
            }

            tracing::warn!(
                card_id = card.id,
                topic = ctx.topic_name,
                "dismissing card with failed generation content"
            );
            match tipcards::delete_with_review(&ctx.state.db, ctx.user_id, card.id).await {
                Ok(image_paths) => {
                    image_store::remove_stored_files(&ctx.state.image_dir, &image_paths).await;
                }
                Err(err) => {
                    tracing::warn!(error = ?err, card_id = card.id, "failed to delete failed generation card");
                    return Ok(false);
                }
            }
        }
        Ok(false)
    }

    /// Spawn a background pending refill for the topic when its queue has hit
    /// the low-water mark. Never blocks or fails the caller.
    async fn maybe_spawn_pending_refill(ctx: &GenerationContext<'_>) {
        let pending = match tipcards::count_pending(
            &ctx.state.db,
            ctx.user_id,
            ctx.topic.id,
            &ctx.topic.tipcard_type,
        )
        .await
        {
            Ok(pending) => pending,
            Err(err) => {
                tracing::warn!(
                    error = ?err,
                    topic = ctx.topic_name,
                    "failed to count pending for refill decision"
                );
                return;
            }
        };
        if !pending_needs_generation(pending) {
            return;
        }
        let Some(arc) = ctx.state.self_arc.get() else {
            tracing::warn!(
                topic = ctx.topic_name,
                "AppState self_arc unset; skipping background refill"
            );
            return;
        };
        Self::spawn_pending_refill(
            arc.clone(),
            ctx.user_id.to_string(),
            ctx.topic.id,
            ctx.topic_name.to_string(),
            ctx.topic.tipcard_type.clone(),
        );
    }

    fn spawn_pending_refill(
        state: std::sync::Arc<AppState>,
        user_id: String,
        topic_id: i64,
        topic_name: String,
        tipcard_type: String,
    ) {
        tokio::spawn(async move {
            let started = std::time::Instant::now();
            match Self::refill_pending_batch(&state, &user_id, topic_id, &topic_name, &tipcard_type)
                .await
            {
                Ok(created) => tracing::info!(
                    user_id,
                    topic = %topic_name,
                    created,
                    duration_ms = started.elapsed().as_millis() as u64,
                    "background pending refill completed"
                ),
                Err(message) => tracing::error!(
                    user_id,
                    topic = %topic_name,
                    error = %message,
                    "background pending refill failed"
                ),
            }
        });
    }

    /// Claim the per-topic refill slot, run the refill, and release it. A claim
    /// already held by another task makes this a no-op so concurrent requests
    /// never start duplicate LLM refills for the same topic.
    async fn refill_pending_batch(
        state: &AppState,
        user_id: &str,
        topic_id: i64,
        topic_name: &str,
        tipcard_type: &str,
    ) -> Result<usize, String> {
        let key = (user_id.to_string(), topic_id);
        {
            let mut claims = state
                .generation_locks
                .lock()
                .map_err(|_| "generation lock poisoned".to_string())?;
            if claims.contains(&key) {
                return Ok(0);
            }
            claims.insert(key.clone());
        }
        let result =
            Self::refill_pending_unlocked(state, user_id, topic_id, topic_name, tipcard_type).await;
        if let Ok(mut claims) = state.generation_locks.lock() {
            claims.remove(&key);
        }
        result
    }

    /// Re-read settings and the topic, generate a fresh batch, and persist it as
    /// pending. Skips when another request refilled the queue while the LLM call
    /// was in flight.
    #[allow(clippy::too_many_arguments)]
    async fn refill_pending_unlocked(
        state: &AppState,
        user_id: &str,
        topic_id: i64,
        topic_name: &str,
        tipcard_type: &str,
    ) -> Result<usize, String> {
        let pending = tipcards::count_pending(&state.db, user_id, topic_id, tipcard_type)
            .await
            .map_err(|err| err.into_status_body().1)?;
        if !pending_needs_generation(pending) {
            return Ok(0);
        }

        let defaults = state
            .settings
            .get_settings()
            .map_err(|err| err.into_status_body().1)?;
        let settings = user_settings::get(&state.db, user_id, defaults)
            .await
            .map_err(|err| err.into_status_body().1)?;
        let topic = topics::find_by_id(&state.db, user_id, topic_id)
            .await
            .map_err(|err| err.into_status_body().1)?
            .ok_or_else(|| "topic no longer exists".to_string())?;

        let grounding = domain::grounding::GroundingStrategy::from_setting(
            topic
                .grounding_strategy
                .as_deref()
                .unwrap_or(&settings.grounding_strategy),
        );
        let llm_reasoning = llm::ReasoningConfig::new(settings.llm_reasoning_effort.clone());
        let grounding_reasoning = llm::ReasoningConfig::new(settings.grounding_reasoning_effort());
        let (generation_model, generation_reasoning) =
            if matches!(grounding, domain::grounding::GroundingStrategy::Factual) {
                (settings.llm_model.as_str(), llm_reasoning)
            } else {
                (settings.grounding_model(), grounding_reasoning)
            };
        let compression_level = topic
            .compression_level
            .as_deref()
            .map(llm::CompressionLevel::from_setting)
            .unwrap_or_else(|| {
                llm::CompressionLevel::from_setting(&settings.llm_compression_level)
            });
        let template = topic
            .prompt_template
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&settings.prompt_template);

        let card_context = context::load_card_context(state, user_id, topic_id, tipcard_type)
            .await
            .map_err(|err| err.1)?;
        let prompt = context::render_generation_prompt(topic_name, template, &card_context);
        let documents: Vec<llm::DocChunk> =
            if matches!(grounding, domain::grounding::GroundingStrategy::Rag) {
                documents::retrieve_chunks(&state.db, user_id, topic_id, topic_name, 12)
                    .await
                    .map_err(|err| err.into_status_body().1)?
                    .into_iter()
                    .map(|chunk| llm::DocChunk { chunk })
                    .collect()
            } else {
                Vec::new()
            };

        let outcome = llm::ground_and_generate(
            grounding,
            llm::GroundingInput {
                topic_name,
                rendered_prompt: &prompt,
                compression_level,
                model: generation_model,
                api_key: &settings.llm_api_key,
                api_base: &settings.llm_base_url,
                reasoning: &generation_reasoning,
                existing_titles: card_context.existing_titles(),
                documents: &documents,
                daily_card_count: REPEATABLE_BATCH.clamp(5, 12) as i64,
                search: llm::SearchConfig {
                    provider: &settings.search_provider,
                    external_key: &settings.search_api_key,
                    base_url: &settings.search_base_url,
                },
            },
        )
        .await?;

        // Another request may have refilled this topic while the LLM call ran.
        let current_pending = tipcards::count_pending(&state.db, user_id, topic_id, tipcard_type)
            .await
            .map_err(|err| err.into_status_body().1)?;
        if !pending_needs_generation(current_pending) {
            return Ok(0);
        }

        let cards = outcome
            .cards()
            .map(|card| tipcards::GeneratedCardParams {
                title: &card.title,
                full_content: &card.full_content,
                compressed_content: &card.compressed_content,
                use_image: card.use_image,
                image_query: &card.image_query,
            })
            .collect::<Vec<_>>();
        tipcards::create_pending_batch_if_needed(
            &state.db,
            user_id,
            topic_id,
            tipcard_type,
            REFILL_LOW_WATER,
            &cards,
        )
        .await
        .map(|ids| ids.len())
        .map_err(|err| err.into_status_body().1)
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

/// Number of cards generated per repeatable-topic load. A large batch keeps the
/// pending queue full so reviews rarely block on a synchronous LLM refill.
const REPEATABLE_BATCH: usize = 12;
/// Refill the pending queue (synchronously or in the background) once it drops
/// to this many cards, so the queue has a buffer before it empties.
const REFILL_LOW_WATER: i64 = 3;

fn pending_needs_generation(pending_count: i64) -> bool {
    pending_count <= REFILL_LOW_WATER
}

#[cfg(test)]
mod generation_queue_tests {
    use super::pending_needs_generation;

    #[test]
    fn bootstraps_empty_queue_and_refills_only_at_low_water_mark() {
        assert!(pending_needs_generation(0));
        assert!(pending_needs_generation(1));
        assert!(pending_needs_generation(2));
        assert!(pending_needs_generation(3));
        assert!(!pending_needs_generation(4));
        assert!(!pending_needs_generation(10));
    }
}
