//! RAG grounding: generate a card using ONLY user-provided document chunks that
//! the service pre-retrieved (FTS5 keyword retrieval).

use crate::llm::cards::{
    ONE_SHOT_FORMAT_INSTRUCTIONS, build_card_from_parsed, parse_generated_card_response,
};
use crate::llm::transport::create_chat_completion;

use super::{GroundingInput, GroundingOutcome, factual_fallback};

pub async fn generate(input: GroundingInput<'_>) -> GroundingOutcome {
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
             {format}",
            base = input.rendered_prompt,
            format = ONE_SHOT_FORMAT_INSTRUCTIONS,
        )
    } else {
        format!(
            "{base}\n\n\
             Use ONLY the provided sources below. Cite the source titles you rely on inside \
             the \"content\". If the sources do not cover the topic, say so plainly rather \
             than inventing facts.\n\n\
             Sources:\n{sources}\n\n\
             {format}",
            base = input.rendered_prompt,
            sources = sources,
            format = ONE_SHOT_FORMAT_INSTRUCTIONS,
        )
    };

    let response = create_chat_completion(
        input.model,
        &prompt,
        input.api_key,
        input.api_base,
        input.reasoning,
        Some(2048),
    )
    .await;

    let parsed = parse_generated_card_response(&response.content);
    if parsed.is_none() {
        tracing::warn!("rag grounding did not return parseable JSON; using raw content");
    }
    let card = build_card_from_parsed(
        parsed,
        &response.content,
        response.usage,
        input.compression_level,
        input.model,
        input.api_key,
        input.api_base,
    )
    .await;

    GroundingOutcome::single(card)
}

fn render_chunks(documents: &[super::DocChunk]) -> String {
    documents
        .iter()
        .enumerate()
        .map(|(idx, doc)| format!("[{}] {}", idx + 1, doc.chunk))
        .collect::<Vec<_>>()
        .join("\n\n")
}
