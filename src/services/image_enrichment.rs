use crate::{
    AppState,
    db::repositories::{image_jobs, image_pool, tipcards, user_settings},
    domain,
    error::{AppError, AppResult},
    image_compress::PreparedImage,
    image_store, llm,
};

const MAX_ATTEMPTS: i64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageJobRun {
    Idle,
    Attached(i64),
    CompletedWithoutImage(i64),
    Retrying(i64),
    Failed(i64),
}

pub async fn process_one(state: &AppState) -> AppResult<ImageJobRun> {
    let Some(claim) = image_jobs::claim_next(&state.db).await? else {
        return Ok(ImageJobRun::Idle);
    };
    let card_id = claim.card_id;
    let result = enrich_claimed_card(state, &claim).await;
    match result {
        Ok(attached) => {
            image_jobs::mark_completed(&state.db, card_id).await?;
            if attached {
                Ok(ImageJobRun::Attached(card_id))
            } else {
                Ok(ImageJobRun::CompletedWithoutImage(card_id))
            }
        }
        Err(message) => {
            image_jobs::mark_retry_or_failed(&state.db, &claim, &message, MAX_ATTEMPTS).await?;
            tracing::warn!(
                card_id,
                attempt = claim.attempts,
                error = %message,
                "card image enrichment did not complete"
            );
            if claim.attempts >= MAX_ATTEMPTS {
                Ok(ImageJobRun::Failed(card_id))
            } else {
                Ok(ImageJobRun::Retrying(card_id))
            }
        }
    }
}

async fn enrich_claimed_card(
    state: &AppState,
    claim: &image_jobs::ImageJobClaim,
) -> Result<bool, String> {
    if !tipcards::list_images(&state.db, &claim.user_id, claim.card_id)
        .await
        .map_err(app_error)?
        .is_empty()
    {
        return Ok(true);
    }

    let card = image_jobs::load_card(&state.db, claim.card_id, &claim.user_id)
        .await
        .map_err(app_error)?;
    if !card.use_image || card.image_query.trim().is_empty() {
        return Ok(false);
    }

    let defaults = state.settings.get_settings().map_err(app_error)?;
    let settings = user_settings::get(&state.db, &claim.user_id, defaults)
        .await
        .map_err(app_error)?;
    let strategy = domain::grounding::ImageStrategy::from_setting(
        card.image_strategy
            .as_deref()
            .unwrap_or(&settings.image_strategy),
    );
    if matches!(strategy, domain::grounding::ImageStrategy::None) {
        return Ok(false);
    }

    let pool = image_pool::list_pool_images(&state.db, &claim.user_id)
        .await
        .map_err(app_error)?;
    let pool_meta = pool
        .iter()
        .map(|row| llm::PoolImageMeta {
            id: row.id,
            name: row.name.clone(),
            description: row.description.clone(),
        })
        .collect::<Vec<_>>();
    let sources = domain::grounding::image_sources_from_setting(&settings.image_sources);
    let (model, reasoning) = if matches!(strategy, domain::grounding::ImageStrategy::Agentic) {
        (
            settings.grounding_model(),
            llm::ReasoningConfig::new(settings.grounding_reasoning_effort()),
        )
    } else {
        (
            settings.llm_model.as_str(),
            llm::ReasoningConfig::new(settings.llm_reasoning_effort.clone()),
        )
    };

    let retrieved = llm::retrieve_image(
        strategy,
        llm::ImageInput {
            topic_name: &card.topic_name,
            card_title: &card.title,
            card_content: &card.full_content,
            image_query: &card.image_query,
            model,
            api_key: &settings.llm_api_key,
            api_base: &settings.llm_base_url,
            reasoning: &reasoning,
            pool: &pool_meta,
            sources: &sources,
            search_api_key: &settings.search_api_key,
            search_base_url: &settings.search_base_url,
            search_provider: &settings.search_provider,
        },
    )
    .await
    .ok_or_else(|| "image strategy returned no usable image".to_string())?;

    let prepared = match retrieved {
        llm::RetrievedImage::Prepared(prepared) => prepared,
        llm::RetrievedImage::Pool(pool_id) => {
            let row = pool
                .iter()
                .find(|row| row.id == pool_id)
                .ok_or_else(|| "selected pool image no longer exists".to_string())?;
            let bytes = tokio::fs::read(state.image_dir.join(&row.storage_path))
                .await
                .map_err(|err| format!("failed to read selected pool image: {err}"))?;
            PreparedImage {
                bytes,
                mime_type: row.mime_type.clone(),
                extension: extension_for_mime(&row.mime_type)
                    .ok_or_else(|| "selected pool image has an unsupported MIME type".to_string())?
                    .to_string(),
            }
        }
    };
    store_prepared(state, &claim.user_id, claim.card_id, prepared).await?;
    tracing::info!(
        card_id = claim.card_id,
        topic_id = card.topic_id,
        topic = card.topic_name,
        "card image enrichment attached an image"
    );
    Ok(true)
}

pub(crate) async fn store_prepared(
    state: &AppState,
    user_id: &str,
    card_id: i64,
    prepared: PreparedImage,
) -> Result<(), String> {
    image_store::replace_card_prepared_image(
        &state.db,
        &state.image_dir,
        user_id,
        card_id,
        prepared,
    )
    .await
    .map_err(|(_, message)| message)
}

fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

fn app_error(error: AppError) -> String {
    error.message()
}
