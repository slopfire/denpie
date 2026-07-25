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
    cards::{GeneratedCard, build_card_from_parsed, parse_card_array},
    compression::CompressionLevel,
    transport::{LlmResponse, ReasoningConfig, TokenUsage},
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
    /// Requested batch size, clamped by every strategy to 5-10 cards.
    pub daily_card_count: i64,
    pub search: SearchConfig<'a>,
}

/// Result of a grounding pass. The service persists every card as pending before
/// promoting the oldest queued card for delivery.
pub struct GroundingOutcome {
    pub primary: GeneratedCard,
    pub pending: Vec<GeneratedCard>,
    pub citations: Vec<String>,
    pub usage: TokenUsage,
}

impl GroundingOutcome {
    pub(crate) fn from_cards(
        mut cards: Vec<GeneratedCard>,
        citations: Vec<String>,
    ) -> Option<Self> {
        if cards.is_empty() {
            return None;
        }
        let parsed_count = cards.len();
        while cards.len() < 5 {
            let mut card = cards[cards.len() % parsed_count].clone();
            card.title = format!("{} {}", card.title, cards.len() + 1);
            card.usage = TokenUsage::default();
            cards.push(card);
        }
        let primary = cards.remove(0);
        let mut usage = primary.usage.clone();
        for card in &cards {
            usage = add_usage(&usage, &card.usage);
        }
        Some(Self {
            primary,
            pending: cards,
            citations,
            usage,
        })
    }

    pub(crate) fn cards(&self) -> impl Iterator<Item = &GeneratedCard> {
        std::iter::once(&self.primary).chain(self.pending.iter())
    }
}

pub(crate) fn batch_size(input: &GroundingInput<'_>) -> usize {
    input.daily_card_count.clamp(5, 10) as usize
}

pub(crate) async fn build_batch(
    response: LlmResponse,
    input: &GroundingInput<'_>,
) -> Vec<GeneratedCard> {
    let parsed = parse_card_array(&response.content)
        .into_iter()
        .take(batch_size(input))
        .collect::<Vec<_>>();
    let mut cards = Vec::with_capacity(parsed.len());
    let mut response_usage = response.usage;
    for item in parsed {
        cards.push(
            build_card_from_parsed(
                Some(item),
                "",
                std::mem::take(&mut response_usage),
                input.compression_level,
                input.model,
                input.api_key,
                input.api_base,
            )
            .await,
        );
    }
    cards
}

pub(crate) fn batch_prompt(input: &GroundingInput<'_>) -> String {
    format!(
        "{base}\n\nWrite {count} distinct, non-overlapping cards for this load.\n\n{format}",
        base = input.rendered_prompt,
        count = batch_size(input),
        format = crate::llm::cards::ARRAY_FORMAT_INSTRUCTIONS,
    )
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
    GroundingOutcome::from_cards(vec![card], Vec::new()).expect("fallback batch is non-empty")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(number: usize) -> GeneratedCard {
        GeneratedCard {
            title: format!("Card {number}"),
            full_content: format!("Full {number}"),
            compressed_content: format!("Compact {number}"),
            use_image: false,
            image_query: String::new(),
            usage: TokenUsage::default(),
        }
    }

    #[test]
    fn partial_model_batch_is_padded_to_five_cards() {
        let outcome = GroundingOutcome::from_cards(vec![card(1), card(2)], Vec::new()).unwrap();
        assert_eq!(outcome.cards().count(), 5);
    }

    #[test]
    fn complete_model_batch_is_not_expanded() {
        let outcome =
            GroundingOutcome::from_cards((1..=8).map(card).collect(), Vec::new()).unwrap();
        assert_eq!(outcome.cards().count(), 8);
    }
}
