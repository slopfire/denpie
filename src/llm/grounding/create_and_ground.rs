//! Create-and-ground: generate a card, then fact-check it against current web
//! sources (provider-native web plugin) or external search snippets.

use crate::llm::cards::{build_card_from_parsed, generate_card, parse_generated_card_response};
use crate::llm::transport::create_chat_completion_grounded;

use super::search::{render_hits, search_external};
use super::{GroundingInput, GroundingOutcome, add_usage};

pub async fn generate(input: GroundingInput<'_>) -> GroundingOutcome {
    // First produce a draft with the normal one-shot path.
    let draft = generate_card(
        input.rendered_prompt,
        input.compression_level,
        input.model,
        input.api_key,
        input.api_base,
        input.reasoning,
    )
    .await;

    // No key → generate_card already returned the offline fallback; nothing to ground.
    if input.api_key.is_empty() {
        return GroundingOutcome::single(draft);
    }

    // Build a fact-check prompt around the draft's full content.
    let (verify_prompt, web_search) = if input.search.provider_native() {
        (
            format!(
                "You are fact-checking a draft tip card about \"{topic}\".\n\n\
                 Draft content:\n{content}\n\n\
                 Verify each claim against current web sources. Rewrite any unsupported or \
                 outdated claim so it is accurate. Keep the tip practical and specific.\n\n\
                 {format}",
                topic = input.topic_name,
                content = draft.full_content,
                format = crate::llm::cards::ONE_SHOT_FORMAT_INSTRUCTIONS,
            ),
            true,
        )
    } else {
        let hits = search_external(&input.search, input.topic_name, 5).await;
        let sources = render_hits(&hits);
        (
            format!(
                "You are fact-checking a draft tip card about \"{topic}\".\n\n\
                 Draft content:\n{content}\n\n\
                 Web sources:\n{sources}\n\n\
                 Verify each claim against the sources above. Rewrite any unsupported or \
                 outdated claim so it is accurate. Keep the tip practical and specific.\n\n\
                 {format}",
                topic = input.topic_name,
                content = draft.full_content,
                sources = sources,
                format = crate::llm::cards::ONE_SHOT_FORMAT_INSTRUCTIONS,
            ),
            false,
        )
    };

    let response = create_chat_completion_grounded(
        input.model,
        &verify_prompt,
        input.api_key,
        input.api_base,
        input.reasoning,
        Some(2048),
        web_search,
    )
    .await;

    let citations = response.citations.clone();
    let base_usage = add_usage(&draft.usage, &response.usage);

    // If the verification response did not parse, keep the draft but preserve usage.
    let parsed = parse_generated_card_response(&response.content);
    if parsed.is_none() {
        tracing::warn!(
            "create_and_ground verification did not return parseable JSON; keeping draft"
        );
        let mut primary = draft;
        primary.usage = base_usage.clone();
        return GroundingOutcome {
            primary,
            pending: Vec::new(),
            citations,
            usage: base_usage,
        };
    }

    let verified = build_card_from_parsed(
        parsed,
        &response.content,
        base_usage,
        input.compression_level,
        input.model,
        input.api_key,
        input.api_base,
    )
    .await;

    let usage = verified.usage.clone();
    GroundingOutcome {
        primary: verified,
        pending: Vec::new(),
        citations,
        usage,
    }
}
