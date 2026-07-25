//! RAG grounding: generate a card using ONLY user-provided document chunks that
//! the service pre-retrieved (FTS5 keyword retrieval).

use crate::llm::cards::ARRAY_FORMAT_INSTRUCTIONS;
use crate::llm::transport::create_chat_completion;

use super::{GroundingInput, GroundingOutcome, batch_size, build_batch, factual_fallback};

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

    let response = create_chat_completion(
        input.model,
        &prompt,
        input.api_key,
        input.api_base,
        input.reasoning,
        Some(2048),
    )
    .await;

    match GroundingOutcome::from_cards(build_batch(response, &input).await, Vec::new()) {
        Some(outcome) => outcome,
        None => {
            tracing::warn!("rag grounding returned no parseable batch; using fallback");
            factual_fallback(&input).await
        }
    }
}

fn render_chunks(documents: &[super::DocChunk]) -> String {
    documents
        .iter()
        .enumerate()
        .map(|(idx, doc)| format!("[{}] {}", idx + 1, doc.chunk))
        .collect::<Vec<_>>()
        .join("\n\n")
}
