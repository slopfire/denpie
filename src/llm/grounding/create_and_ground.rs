//! Create-and-ground: generate a batch, then fact-check it against current web
//! sources (provider-native web plugin) or external search snippets.

use crate::llm::cards::ARRAY_FORMAT_INSTRUCTIONS;
use crate::llm::transport::create_chat_completion_grounded;

use super::search::{render_hits, search_external};
use super::{
    GroundingInput, GroundingOutcome, add_usage, batch_size, build_batch, factual_fallback,
    generate_batch_with_retry,
};

pub async fn generate(input: GroundingInput<'_>) -> Result<GroundingOutcome, String> {
    if input.api_key.is_empty() {
        return factual_fallback(&input).await;
    }

    let mut drafts = generate_batch_with_retry(&input, 8192, false, true).await?;
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
    let (verify_prompt, web_search) = if input.search.provider_native() {
        (
            format!(
                "You are fact-checking {count} draft tip cards about \"{topic}\".\n\n\
                 Draft cards:\n{content}\n\n\
                 Verify each claim against current web sources. Rewrite any unsupported or \
                 outdated claim so it is accurate. Return exactly {count} distinct cards.\n\n\
                 {format}",
                count = batch_size(&input),
                topic = input.topic_name,
                content = draft_text,
                format = ARRAY_FORMAT_INSTRUCTIONS,
            ),
            true,
        )
    } else {
        let hits = search_external(&input.search, input.topic_name, 5).await;
        let sources = render_hits(&hits);
        (
            format!(
                "You are fact-checking {count} draft tip cards about \"{topic}\".\n\n\
                 Draft cards:\n{content}\n\n\
                 Web sources:\n{sources}\n\n\
                 Verify each claim against the sources above. Rewrite any unsupported or \
                 outdated claim so it is accurate. Return exactly {count} distinct cards.\n\n\
                 {format}",
                count = batch_size(&input),
                topic = input.topic_name,
                content = draft_text,
                sources = sources,
                format = ARRAY_FORMAT_INSTRUCTIONS,
            ),
            false,
        )
    };

    let mut citations = Vec::new();
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
                "create_and_ground verification request failed; keeping drafts"
            );
            if attempt < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                continue;
            }
            break;
        }
        citations = response.citations.clone();
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
            "create_and_ground verification returned no parseable batch; keeping drafts"
        );
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
    }

    if let Some(mut outcome) = GroundingOutcome::from_cards(verified, citations.clone()) {
        // `outcome.usage` already includes the verification response usage via
        // `build_batch`; add the draft pass on top.
        outcome.usage = add_usage(&draft_usage, &outcome.usage);
        return Ok(outcome);
    }

    // Verification failed or was unparseable: ship the drafts with the
    // verification token usage attributed to the first card.
    drafts[0].usage = match verify_usage {
        Some(verify_usage) => add_usage(&drafts[0].usage, &verify_usage),
        None => drafts[0].usage.clone(),
    };
    Ok(GroundingOutcome::from_cards(drafts, citations).expect("draft batch is non-empty"))
}
