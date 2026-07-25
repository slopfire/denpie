//! Create-and-ground: generate a batch, then fact-check it against current web
//! sources (provider-native web plugin) or external search snippets.

use crate::llm::cards::ARRAY_FORMAT_INSTRUCTIONS;
use crate::llm::transport::{create_chat_completion, create_chat_completion_grounded};

use super::search::{render_hits, search_external};
use super::{
    GroundingInput, GroundingOutcome, add_usage, batch_prompt, batch_size, build_batch,
    factual_fallback,
};

pub async fn generate(input: GroundingInput<'_>) -> GroundingOutcome {
    if input.api_key.is_empty() {
        return factual_fallback(&input).await;
    }

    let draft_response = create_chat_completion(
        input.model,
        &batch_prompt(&input),
        input.api_key,
        input.api_base,
        input.reasoning,
        Some(4096),
    )
    .await;
    let mut drafts = build_batch(draft_response, &input).await;
    if drafts.is_empty() {
        tracing::warn!("create_and_ground draft returned no parseable batch; using fallback");
        return factual_fallback(&input).await;
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
    let verify_usage = response.usage.clone();
    let verified = build_batch(response, &input).await;
    if let Some(mut outcome) = GroundingOutcome::from_cards(verified, citations.clone()) {
        outcome.usage = add_usage(&draft_usage, &outcome.usage);
        return outcome;
    }

    tracing::warn!("create_and_ground verification returned no parseable batch; keeping drafts");
    drafts[0].usage = add_usage(&drafts[0].usage, &verify_usage);
    GroundingOutcome::from_cards(drafts, citations).expect("draft batch is non-empty")
}
