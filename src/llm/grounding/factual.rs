//! Factual generation: one model call produces a pending-card batch.

use crate::llm::transport::create_chat_completion;

use super::{GroundingInput, GroundingOutcome, batch_prompt, build_batch, factual_fallback};

pub async fn generate(input: GroundingInput<'_>) -> GroundingOutcome {
    if input.api_key.is_empty() {
        return factual_fallback(&input).await;
    }
    let response = create_chat_completion(
        input.model,
        &batch_prompt(&input),
        input.api_key,
        input.api_base,
        input.reasoning,
        Some(4096),
    )
    .await;
    match GroundingOutcome::from_cards(build_batch(response, &input).await, Vec::new()) {
        Some(outcome) => outcome,
        None => {
            tracing::warn!("factual generation returned no parseable batch; using fallback");
            factual_fallback(&input).await
        }
    }
}
