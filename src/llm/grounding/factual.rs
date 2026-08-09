//! Factual generation: one model call produces a pending-card batch.

use super::{GroundingInput, GroundingOutcome, factual_fallback, generate_batch_with_retry};

pub async fn generate(input: GroundingInput<'_>) -> Result<GroundingOutcome, String> {
    if input.api_key.is_empty() {
        return factual_fallback(&input).await;
    }
    let cards = generate_batch_with_retry(&input, 8192, false, true).await?;
    Ok(GroundingOutcome::from_cards(cards, Vec::new()).expect("non-empty batch after retry check"))
}
