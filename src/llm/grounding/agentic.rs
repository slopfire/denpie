//! Agentic grounding: research a topic (avoiding existing cards), generate many
//! cards at once; the first is served now, the rest become a pending backlog.

use crate::llm::cards::{ARRAY_FORMAT_INSTRUCTIONS, build_card_from_parsed, parse_card_array};
use crate::llm::transport::{TokenUsage, create_chat_completion_grounded};

use super::search::{render_hits, search_external};
use super::{GroundingInput, GroundingOutcome, factual_fallback};

const MIN_CARDS: i64 = 5;
const MAX_CARDS: i64 = 12;

pub async fn generate(input: GroundingInput<'_>) -> Result<GroundingOutcome, String> {
    tracing::info!(
        topic = input.topic_name,
        model = input.model,
        requested_cards = input.daily_card_count,
        existing_titles = input.existing_titles.len(),
        provider_native_search = input.search.provider_native(),
        "agentic grounding started"
    );
    // No key → fall back to the offline single-card path.
    if input.api_key.is_empty() {
        tracing::warn!(
            topic = input.topic_name,
            "agentic grounding has no LLM API key; using factual fallback"
        );
        return factual_fallback(&input).await;
    }

    let n = input.daily_card_count.clamp(MIN_CARDS, MAX_CARDS);
    let avoid = render_avoid_list(input.existing_titles);

    let (research_prompt, web_search) = if input.search.provider_native() {
        (
            build_prompt(input.rendered_prompt, input.topic_name, n, &avoid, None),
            true,
        )
    } else {
        let hits = search_external(&input.search, input.topic_name, 5).await;
        tracing::info!(
            topic = input.topic_name,
            search_hits = hits.len(),
            "agentic grounding external research completed"
        );
        let sources = render_hits(&hits);
        (
            build_prompt(
                input.rendered_prompt,
                input.topic_name,
                n,
                &avoid,
                Some(&sources),
            ),
            false,
        )
    };

    tracing::info!(
        topic = input.topic_name,
        cards_requested = n,
        prompt_len = research_prompt.len(),
        provider_native_search = web_search,
        "agentic grounding requesting researched card batch"
    );

    let mut last_error = String::new();
    let mut citations = Vec::new();
    let mut parsed = Vec::new();
    let mut response_usage = TokenUsage::default();
    for attempt in 1..=2 {
        let started = std::time::Instant::now();
        let response = create_chat_completion_grounded(
            input.model,
            &research_prompt,
            input.api_key,
            input.api_base,
            input.reasoning,
            Some(8192),
            web_search,
            true,
        )
        .await;
        let duration_ms = started.elapsed().as_millis() as u64;

        tracing::info!(
            topic = input.topic_name,
            attempt,
            duration_ms,
            content_len = response.content.len(),
            citations = response.citations.len(),
            prompt_tokens = response.usage.prompt_tokens,
            completion_tokens = response.usage.completion_tokens,
            total_tokens = response.usage.total_tokens,
            "agentic grounding research response received"
        );

        if response.is_error {
            last_error = format!("LLM request failed: {}", response.content);
            tracing::warn!(
                topic = input.topic_name,
                attempt,
                duration_ms,
                error = %last_error,
                "agentic grounding research request failed"
            );
            if attempt < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                continue;
            }
            return Err(last_error);
        }

        citations = response.citations.clone();
        response_usage = response.usage.clone();
        parsed = parse_card_array(&response.content)
            .into_iter()
            .take(n as usize)
            .collect::<Vec<_>>();
        tracing::info!(
            topic = input.topic_name,
            attempt,
            parsed_cards = parsed.len(),
            "agentic grounding parsed researched card batch"
        );

        if !parsed.is_empty() {
            break;
        }
        last_error = format!(
            "model returned no parseable card JSON: {}",
            crate::llm::transport::content_snippet(&response.content, 200)
        );
        tracing::warn!(
            topic = input.topic_name,
            attempt,
            duration_ms,
            content_len = response.content.len(),
            error = %last_error,
            "agentic grounding produced no parseable cards"
        );
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
    }
    if parsed.is_empty() {
        return Err(last_error);
    }

    // Build each card. The array response's token usage is attributed to the
    // first card; subsequent cards carry only their own compression usage.
    let mut cards = Vec::with_capacity(parsed.len());
    let mut usage_budget = response_usage;
    for (index, item) in parsed.into_iter().enumerate() {
        let base_usage = std::mem::take(&mut usage_budget);
        let card = build_card_from_parsed(
            Some(item),
            "",
            base_usage,
            input.compression_level,
            input.model,
            input.api_key,
            input.api_base,
        )
        .await;
        tracing::info!(
            topic = input.topic_name,
            card_number = index + 1,
            title = card.title,
            "agentic grounding built card"
        );
        cards.push(card);
    }

    let outcome = GroundingOutcome::from_cards(cards, citations.clone())
        .expect("non-empty after is_empty check");

    tracing::info!(
        topic = input.topic_name,
        pending_cards = outcome.pending.len(),
        citations = citations.len(),
        total_tokens = outcome.usage.total_tokens,
        "agentic grounding completed"
    );

    Ok(outcome)
}

fn build_prompt(
    rendered_prompt: &str,
    topic: &str,
    n: i64,
    avoid: &str,
    sources: Option<&str>,
) -> String {
    let sources_block = match sources {
        Some(sources) if !sources.is_empty() => {
            format!("\n\nWeb sources to ground your facts:\n{sources}\n")
        }
        _ => String::new(),
    };
    format!(
        "Apply this learner-specific generation brief to every card:\n{rendered_prompt}\n\n\
         Research the topic \"{topic}\" and write {n} genuinely useful, distinct daily tip cards.\n\
         Each card must be practical, specific, accurate, and worth saving.\n\
         {avoid}{sources_block}\n\
         {format}",
        format = ARRAY_FORMAT_INSTRUCTIONS,
    )
}

fn render_avoid_list(existing_titles: &[String]) -> String {
    if existing_titles.is_empty() {
        return String::new();
    }
    let list = existing_titles
        .iter()
        .enumerate()
        .map(|(idx, title)| format!("{}. {}", idx + 1, title))
        .collect::<Vec<_>>()
        .join("\n");
    format!("Do NOT duplicate these existing card titles or ideas:\n{list}\n")
}

#[cfg(test)]
mod tests {
    use super::build_prompt;

    #[test]
    fn batch_prompt_keeps_learner_feedback() {
        let prompt = build_prompt(
            "Known: basic kana. Too difficult: literary kanji.",
            "Japanese",
            3,
            "",
            None,
        );

        assert!(prompt.contains("Known: basic kana"));
        assert!(prompt.contains("Too difficult: literary kanji"));
        assert!(prompt.contains("write 3"));
    }
}
