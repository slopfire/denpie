//! Factual grounding: no external grounding, current single-shot behavior.

use crate::llm::cards::generate_card;

use super::{GroundingInput, GroundingOutcome};

pub async fn generate(input: GroundingInput<'_>) -> GroundingOutcome {
    let card = generate_card(
        input.rendered_prompt,
        input.compression_level,
        input.model,
        input.api_key,
        input.api_base,
        input.reasoning,
    )
    .await;
    GroundingOutcome::single(card)
}
