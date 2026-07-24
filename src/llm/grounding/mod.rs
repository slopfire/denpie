//! Pluggable grounding subsystem: controls how/where the LLM sources facts when
//! generating a tip card. Pure orchestration — SQL stays in repositories; the
//! caller passes already-retrieved documents and search config.

mod agentic;
mod create_and_ground;
mod factual;
mod rag;
pub mod search;

use crate::domain::grounding::GroundingStrategy;
use crate::llm::{
    cards::GeneratedCard,
    compression::CompressionLevel,
    transport::{ReasoningConfig, TokenUsage},
};

#[allow(unused_imports)]
pub use search::{SearchConfig, SearchHit};

/// A retrieved document chunk for RAG grounding.
#[derive(Clone, Debug)]
pub struct DocChunk {
    pub chunk: String,
}

/// Inputs to a grounding strategy. Borrows everything; the caller owns the data.
pub struct GroundingInput<'a> {
    pub topic_name: &'a str,
    pub rendered_prompt: &'a str,
    pub compression_level: CompressionLevel,
    pub model: &'a str,
    pub api_key: &'a str,
    pub api_base: &'a str,
    pub reasoning: &'a ReasoningConfig,
    /// Existing card titles to avoid duplicating (used by agentic research).
    pub existing_titles: &'a [String],
    /// Pre-retrieved document chunks (used by rag).
    pub documents: &'a [DocChunk],
    /// Daily card count for the topic, clamped by agentic to bound backlog size.
    pub daily_card_count: i64,
    pub search: SearchConfig<'a>,
}

/// Result of a grounding pass: the card to serve now, plus an optional backlog of
/// pending cards (agentic), citations, and accumulated token usage.
pub struct GroundingOutcome {
    pub primary: GeneratedCard,
    pub pending: Vec<GeneratedCard>,
    pub citations: Vec<String>,
    pub usage: TokenUsage,
}

impl GroundingOutcome {
    /// Wrap a single generated card with no backlog/citations. Usage comes from
    /// the card itself.
    pub(crate) fn single(card: GeneratedCard) -> Self {
        let usage = card.usage.clone();
        Self {
            primary: card,
            pending: Vec::new(),
            citations: Vec::new(),
            usage,
        }
    }
}

/// Dispatch to the configured grounding strategy. This `match` is the single
/// extension point: a new strategy = new enum arm + new arm here + new module.
pub async fn ground_and_generate(
    strategy: GroundingStrategy,
    input: GroundingInput<'_>,
) -> GroundingOutcome {
    match strategy {
        GroundingStrategy::Factual => factual::generate(input).await,
        GroundingStrategy::CreateAndGround => create_and_ground::generate(input).await,
        GroundingStrategy::Agentic => agentic::generate(input).await,
        GroundingStrategy::Rag => rag::generate(input).await,
    }
}

/// Sum token usage across two calls (mirrors the accumulation in `generate_card`).
pub(crate) fn add_usage(a: &TokenUsage, b: &TokenUsage) -> TokenUsage {
    TokenUsage {
        prompt_tokens: a.prompt_tokens + b.prompt_tokens,
        completion_tokens: a.completion_tokens + b.completion_tokens,
        total_tokens: a.total_tokens + b.total_tokens,
    }
}

/// Generate a single ungrounded card from the input. Used when a richer strategy
/// has nothing to work with (no API key, unparseable model output).
pub(crate) async fn factual_fallback(input: &GroundingInput<'_>) -> GroundingOutcome {
    let card = crate::llm::cards::generate_card(
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
