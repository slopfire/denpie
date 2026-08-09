//! RAG grounding: generate a card using ONLY user-provided document chunks that
//! the service pre-retrieved (PostgreSQL full-text retrieval).

use crate::llm::cards::ARRAY_FORMAT_INSTRUCTIONS;
use crate::llm::transport::create_chat_completion_json;

use super::{GroundingInput, GroundingOutcome, batch_size, factual_fallback};

pub async fn generate(input: GroundingInput<'_>) -> Result<GroundingOutcome, String> {
    // No key → offline fallback card.
    if input.api_key.is_empty() {
        return factual_fallback(&input).await;
    }

    let sources = render_chunks(input.documents);
    let prompt = if sources.is_empty() {
        format!(
            "{base}\n\n\
             Note: no grounding documents are available for this topic. Write the best \
             accurate tip you can from general knowledge, and do not fabricate citations.\n\n\
             Write {count} distinct cards for this load.\n\n{format}",
            base = input.rendered_prompt,
            count = batch_size(&input),
            format = ARRAY_FORMAT_INSTRUCTIONS,
        )
    } else {
        format!(
            "{base}\n\n\
             Use ONLY the provided sources below. Cite the source titles you rely on inside \
             the \"content\". If the sources do not cover the topic, say so plainly rather \
             than inventing facts.\n\n\
             Sources:\n{sources}\n\n\
             Write {count} distinct cards for this load.\n\n{format}",
            base = input.rendered_prompt,
            sources = sources,
            count = batch_size(&input),
            format = ARRAY_FORMAT_INSTRUCTIONS,
        )
    };

    let mut last_error = String::new();
    for attempt in 1..=2 {
        let started = std::time::Instant::now();
        let response = create_chat_completion_json(
            input.model,
            &prompt,
            input.api_key,
            input.api_base,
            input.reasoning,
            Some(8192),
        )
        .await;
        let duration_ms = started.elapsed().as_millis() as u64;
        if response.is_error {
            last_error = format!("LLM request failed: {}", response.content);
            tracing::warn!(
                topic = input.topic_name,
                attempt,
                duration_ms,
                error = %last_error,
                "rag grounding request failed"
            );
            if attempt < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                continue;
            }
            return Err(last_error);
        }
        let snippet = crate::llm::transport::content_snippet(&response.content, 200);
        let cards = super::build_batch(response, &input).await;
        if let Some(outcome) = GroundingOutcome::from_cards(cards, Vec::new()) {
            return Ok(outcome);
        }
        last_error = format!("model returned no parseable card JSON: {snippet}");
        tracing::warn!(
            topic = input.topic_name,
            attempt,
            duration_ms,
            error = %last_error,
            "rag grounding response was not parseable JSON"
        );
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
    }
    Err(last_error)
}

fn render_chunks(documents: &[super::DocChunk]) -> String {
    documents
        .iter()
        .enumerate()
        .map(|(idx, doc)| format!("[{}] {}", idx + 1, doc.chunk))
        .collect::<Vec<_>>()
        .join("\n\n")
}
