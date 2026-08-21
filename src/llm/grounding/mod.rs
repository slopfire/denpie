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
    transport::{
        LlmResponse, ReasoningConfig, TokenUsage, content_snippet, create_chat_completion,
        create_chat_completion_grounded, create_chat_completion_json,
    },
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
    /// Requested batch size, clamped by every strategy to 5-12 cards.
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
    input.daily_card_count.clamp(5, 12) as usize
}

/// How many times a batch generation retries when the model returns an error or
/// no parseable JSON batch. Transport-level failures are already retried inside
/// [`create_chat_completion`]; this retry covers model output quality.
const BATCH_MAX_ATTEMPTS: usize = 2;

/// Generate a card batch from the shared [`batch_prompt`], retrying once when
/// the response is an error or contains no parseable card batch. `json_object`
/// asks the provider for strict JSON output.
pub(crate) async fn generate_batch_with_retry(
    input: &GroundingInput<'_>,
    max_tokens: u32,
    web_search: bool,
    json_object: bool,
) -> Result<Vec<GeneratedCard>, String> {
    let prompt = batch_prompt(input);
    let mut last_error = String::new();
    for attempt in 1..=BATCH_MAX_ATTEMPTS {
        let started = std::time::Instant::now();
        let response = if web_search {
            create_chat_completion_grounded(
                input.model,
                &prompt,
                input.api_key,
                input.api_base,
                input.reasoning,
                Some(max_tokens),
                true,
                json_object,
            )
            .await
        } else if json_object {
            create_chat_completion_json(
                input.model,
                &prompt,
                input.api_key,
                input.api_base,
                input.reasoning,
                Some(max_tokens),
            )
            .await
        } else {
            create_chat_completion(
                input.model,
                &prompt,
                input.api_key,
                input.api_base,
                input.reasoning,
                Some(max_tokens),
            )
            .await
        };
        let duration_ms = started.elapsed().as_millis() as u64;
        if response.is_error {
            last_error = format!("LLM request failed: {}", response.content);
            tracing::warn!(
                topic = input.topic_name,
                attempt,
                duration_ms,
                error = %last_error,
                "grounding batch request failed"
            );
            if attempt < BATCH_MAX_ATTEMPTS {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                continue;
            }
            return Err(last_error);
        }
        let snippet = content_snippet(&response.content, 200);
        let content_len = response.content.len();
        let cards = build_batch(response, input).await;
        if !cards.is_empty() {
            return Ok(cards);
        }
        last_error = format!("model returned no parseable card JSON: {snippet}");
        tracing::warn!(
            topic = input.topic_name,
            attempt,
            duration_ms,
            content_len,
            error = %last_error,
            "grounding batch response was not parseable JSON"
        );
        if attempt < BATCH_MAX_ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
    }
    Err(last_error)
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
    crate::llm::cards::assemble_array_prompt(input.rendered_prompt, batch_size(input))
}

/// Dispatch to the configured grounding strategy. This `match` is the single
/// extension point: a new strategy = new enum arm + new arm here + new module.
/// Returns an error message when generation could not produce any usable card
/// (transport failure after retries, or persistently unparseable model output).
pub async fn ground_and_generate(
    strategy: GroundingStrategy,
    input: GroundingInput<'_>,
) -> Result<GroundingOutcome, String> {
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
/// has nothing to work with (no API key) or as a last resort. Errors propagate
/// when even the single-card path fails.
pub(crate) async fn factual_fallback(
    input: &GroundingInput<'_>,
) -> Result<GroundingOutcome, String> {
    let card = crate::llm::cards::generate_card(
        input.rendered_prompt,
        input.compression_level,
        input.model,
        input.api_key,
        input.api_base,
        input.reasoning,
    )
    .await?;
    Ok(GroundingOutcome::from_cards(vec![card], Vec::new()).expect("fallback batch is non-empty"))
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
