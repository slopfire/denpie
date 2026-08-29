use crate::{
    AppState,
    context::CardContext,
    db::repositories::{documents, tipcards, token_usage, topics, user_settings},
    error::{AppError, AppResult},
    llm::{
        self, PromptEnhanceInput, PromptEnhanceSuggestion, ReasoningConfig, suggest_prompt_template,
    },
};

const HISTORY_LIMIT: i64 = 80;

struct PromptEnhanceTarget {
    topic_name: String,
    current_prompt: String,
    grounding_strategy: String,
    grounding_model: String,
    grounding_effort: String,
    image_strategy: String,
    history: CardContext,
    has_documents: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PromptEnhanceService;

impl PromptEnhanceService {
    pub async fn suggest(
        state: &AppState,
        user_id: &str,
        topic_id: i64,
    ) -> AppResult<PromptEnhanceSuggestion> {
        let defaults = state.settings.get_settings()?;
        let settings = user_settings::get(&state.db, user_id, defaults).await?;
        let target = if topic_id == 0 {
            load_global_target(state, user_id, &settings).await?
        } else {
            load_topic_target(state, user_id, topic_id, &settings).await?
        };

        let reasoning = ReasoningConfig::new(settings.llm_reasoning_effort.clone());
        let input = PromptEnhanceInput {
            topic_name: &target.topic_name,
            current_prompt: &target.current_prompt,
            current_grounding_strategy: &target.grounding_strategy,
            current_grounding_model: &target.grounding_model,
            current_grounding_reasoning_effort: &target.grounding_effort,
            current_image_strategy: &target.image_strategy,
            history: &target.history,
            has_documents: target.has_documents,
        };
        let response = suggest_prompt_template(
            &input,
            &settings.llm_model,
            &settings.llm_api_key,
            &settings.llm_base_url,
            &reasoning,
        )
        .await;
        token_usage::insert(
            &state.db,
            user_id,
            &settings.llm_model,
            "prompt_enhance",
            &response.usage,
        )
        .await?;
        llm::prompt_enhance::decode_suggestion(&response.content)
            .ok_or_else(|| AppError::Validation("Could not read the prompt suggestion".to_string()))
    }
}

async fn load_global_target(
    state: &AppState,
    user_id: &str,
    settings: &crate::config::Settings,
) -> AppResult<PromptEnhanceTarget> {
    let rows = tipcards::list_history_titles_for_user(&state.db, user_id, HISTORY_LIMIT).await?;
    let links = documents::list_document_topic_links(&state.db, user_id).await?;
    Ok(PromptEnhanceTarget {
        topic_name: String::new(),
        current_prompt: llm::effective_prompt_template(None, &settings.prompt_template).to_string(),
        grounding_strategy: settings.grounding_strategy.clone(),
        grounding_model: settings.llm_grounding_model.clone(),
        grounding_effort: settings.llm_grounding_reasoning_effort.clone(),
        image_strategy: settings.image_strategy.clone(),
        history: CardContext::from_title_records(rows),
        has_documents: !links.is_empty(),
    })
}

async fn load_topic_target(
    state: &AppState,
    user_id: &str,
    topic_id: i64,
    settings: &crate::config::Settings,
) -> AppResult<PromptEnhanceTarget> {
    let topic = topics::find_by_id(&state.db, user_id, topic_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Topic not found".to_string()))?;
    let rows = tipcards::list_history_titles_for_topic(&state.db, user_id, topic_id, HISTORY_LIMIT)
        .await?;
    let links = documents::list_document_topic_links(&state.db, user_id).await?;
    let has_documents = links.iter().any(|link| link.topic_id == topic_id);
    Ok(PromptEnhanceTarget {
        topic_name: topic.name,
        current_prompt: llm::effective_prompt_template(
            topic.prompt_template.as_deref(),
            &settings.prompt_template,
        )
        .to_string(),
        grounding_strategy: override_or(&topic.grounding_strategy, &settings.grounding_strategy),
        grounding_model: override_or(&topic.grounding_model, &settings.llm_grounding_model),
        grounding_effort: override_or(
            &topic.grounding_reasoning_effort,
            &settings.llm_grounding_reasoning_effort,
        ),
        image_strategy: override_or(&topic.image_strategy, &settings.image_strategy),
        history: CardContext::from_title_records(rows),
        has_documents,
    })
}

fn override_or(topic_value: &Option<String>, inherited: &str) -> String {
    topic_value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(inherited)
        .to_string()
}
