//! Create-and-ground: generate a batch, then fact-check it against current web
//! sources (provider-native web plugin) or external search snippets.

use crate::domain::grounding::GroundingSourceMode;
use crate::llm::cards::ARRAY_FORMAT_INSTRUCTIONS;
use crate::llm::transport::create_chat_completion_grounded;

use super::search::{render_hits, search_external};
use super::{
    GroundingInput, GroundingOutcome, add_usage, batch_size, build_batch, generate_batch_with_retry,
};

pub async fn generate(input: GroundingInput<'_>) -> Result<GroundingOutcome, String> {
    if input.api_key.is_empty() {
        return Err("fact checking requires an LLM API key".to_string());
    }

    let drafts = generate_batch_with_retry(&input, 8192, false, true).await?;
    if drafts.is_empty() {
        return Err("create_and_ground draft returned no parseable batch".to_string());
    }
    let draft_usage = drafts.iter().fold(Default::default(), |usage, card| {
        add_usage(&usage, &card.usage)
    });

    let draft_text = drafts
        .iter()
        .enumerate()
        .map(|(index, card)| format!("{}. {}\n{}", index + 1, card.title, card.full_content))
        .collect::<Vec<_>>()
        .join("\n\n");
    let document_sources = input
        .documents
        .iter()
        .enumerate()
        .map(|(index, document)| format!("[D{}] {}: {}", index + 1, document.title, document.chunk))
        .collect::<Vec<_>>()
        .join("\n\n");
    let (verify_prompt, web_search, external_citations) = if input.source_mode
        == GroundingSourceMode::Documents
    {
        (
            format!(
                "You are fact-checking {count} draft tip cards about \"{topic}\".\n\n\
                 Draft cards:\n{content}\n\nDocument evidence:\n{documents}\n\n\
                 Verify every claim using only the document evidence. Rewrite or remove unsupported \
                 claims. Return exactly {count} distinct cards.\n\n{format}",
                count = batch_size(&input),
                topic = input.topic_name,
                content = draft_text,
                documents = document_sources,
                format = ARRAY_FORMAT_INSTRUCTIONS,
            ),
            false,
            Vec::new(),
        )
    } else if input.search.provider_native() {
        (
            format!(
                "You are fact-checking {count} draft tip cards about \"{topic}\".\n\n\
                 Draft cards:\n{content}\n\nDocument evidence:\n{documents}\n\n\
                 Verify each claim against current web sources. Rewrite any unsupported or \
                 outdated claim so it is accurate. Return exactly {count} distinct cards.\n\n\
                 {format}",
                count = batch_size(&input),
                topic = input.topic_name,
                content = draft_text,
                documents = document_sources,
                format = ARRAY_FORMAT_INSTRUCTIONS,
            ),
            true,
            Vec::new(),
        )
    } else {
        let hits = search_external(&input.search, input.topic_name).await?;
        if hits.is_empty() {
            return Err("web search returned no evidence".to_string());
        }
        let sources = render_hits(&hits);
        let citations = hits.iter().map(|hit| hit.url.clone()).collect();
        (
            format!(
                "You are fact-checking {count} draft tip cards about \"{topic}\".\n\n\
                 Draft cards:\n{content}\n\n\
                 Web sources:\n{sources}\n\nDocument evidence:\n{documents}\n\n\
                 Verify each claim against the sources above. Rewrite any unsupported or \
                 outdated claim so it is accurate. Return exactly {count} distinct cards.\n\n\
                 {format}",
                count = batch_size(&input),
                topic = input.topic_name,
                content = draft_text,
                sources = sources,
                documents = document_sources,
                format = ARRAY_FORMAT_INSTRUCTIONS,
            ),
            false,
            citations,
        )
    };

    let mut citations = external_citations;
    let mut verify_usage = None;
    let mut verified = Vec::new();
    for attempt in 1..=2 {
        let started = std::time::Instant::now();
        let response = create_chat_completion_grounded(
            input.model,
            &verify_prompt,
            input.api_key,
            input.api_base,
            input.reasoning,
            Some(8192),
            web_search,
            true,
        )
        .await;
        let duration_ms = started.elapsed().as_millis() as u64;
        if response.is_error {
            tracing::warn!(
                topic = input.topic_name,
                attempt,
                duration_ms,
                error = %response.content,
                "create_and_ground verification request failed"
            );
            if attempt < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                continue;
            }
            break;
        }
        if !response.citations.is_empty() {
            citations = response.citations.clone();
        }
        verify_usage = Some(response.usage.clone());
        let snippet = crate::llm::transport::content_snippet(&response.content, 200);
        verified = build_batch(response, &input).await;
        if !verified.is_empty() {
            break;
        }
        tracing::warn!(
            topic = input.topic_name,
            attempt,
            duration_ms,
            error = %snippet,
            "create_and_ground verification returned no parseable batch"
        );
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
    }

    if let Some(mut outcome) = GroundingOutcome::from_cards(verified, citations.clone()) {
        if input.source_mode.uses_web() && input.search.provider_native() && citations.is_empty() {
            return Err("native web verification returned no citations".to_string());
        }
        // `outcome.usage` already includes the verification response usage via
        // `build_batch`; add the draft pass on top.
        outcome.usage = add_usage(&draft_usage, &outcome.usage);
        return Ok(outcome);
    }

    let _ = verify_usage;
    Err("thorough verification failed or returned no parseable card batch".to_string())
}
