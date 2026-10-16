//! Standard grounding: retrieve once, then generate once.

use super::search::{render_hits, search_external};
use super::{GroundingInput, GroundingOutcome, generate_batch_with_retry};
use crate::domain::grounding::GroundingSourceMode;
use crate::llm::transport::create_chat_completion_grounded;

pub async fn generate(input: GroundingInput<'_>) -> Result<GroundingOutcome, String> {
    if input.api_key.is_empty() {
        return Err("grounded generation requires an LLM API key".to_string());
    }
    if input.source_mode == GroundingSourceMode::Documents {
        return super::rag::generate(input).await;
    }

    if input.search.provider_native() {
        let response = create_chat_completion_grounded(
            input.model,
            &super::batch_prompt(&input),
            input.api_key,
            input.api_base,
            input.reasoning,
            Some(8192),
            true,
            true,
        )
        .await;
        if response.is_error {
            return Err(format!("native web search failed: {}", response.content));
        }
        let citations = response.citations.clone();
        if citations.is_empty() {
            return Err("native web search returned no citations".to_string());
        }
        let cards = super::build_batch(response, &input).await;
        return GroundingOutcome::from_cards(cards, citations)
            .ok_or_else(|| "native web search returned no parseable cards".to_string());
    }

    let hits = search_external(&input.search, input.topic_name).await?;
    if hits.is_empty() {
        return Err("web search returned no evidence".to_string());
    }
    let citations = hits.iter().map(|hit| hit.url.clone()).collect::<Vec<_>>();
    let prompt = format!(
        "{}\n\nWeb evidence:\n{}",
        input.rendered_prompt,
        render_hits(&hits)
    );
    let grounded_input = GroundingInput {
        topic_name: input.topic_name,
        rendered_prompt: &prompt,
        compression_level: input.compression_level,
        model: input.model,
        api_key: input.api_key,
        api_base: input.api_base,
        reasoning: input.reasoning,
        existing_titles: input.existing_titles,
        documents: input.documents,
        daily_card_count: input.daily_card_count,
        search: input.search.clone(),
        source_mode: input.source_mode,
        behavior: input.behavior,
        document_searcher: input.document_searcher,
    };
    let cards = generate_batch_with_retry(&grounded_input, 8192, false, true).await?;
    GroundingOutcome::from_cards(cards, citations)
        .ok_or_else(|| "web-grounded generation returned no cards".to_string())
}
