//! Bounded agentic grounding: plan several queries, retrieve evidence, inspect
//! coverage, run at most one follow-up round, then generate from the evidence.

use std::collections::HashSet;

use serde::Deserialize;

use crate::domain::grounding::GroundingSourceMode;
use crate::llm::transport::{
    TokenUsage, create_chat_completion_grounded, create_chat_completion_json,
};

use super::search::{render_hits, search_external};
use super::{DocChunk, GroundingInput, GroundingOutcome, add_usage, generate_batch_with_retry};

const INITIAL_QUERY_LIMIT: usize = 3;
const FOLLOW_UP_QUERY_LIMIT: usize = 2;
const EVIDENCE_LIMIT: usize = 24;
const BASE_MAX_TOKENS: u32 = 8192;
const MAX_TOKENS_CEILING: u32 = 32768;

#[derive(Debug, Deserialize)]
struct QueryPlan {
    queries: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GapPlan {
    missing_queries: Vec<String>,
}

#[derive(Clone, Debug)]
struct EvidenceNote {
    title: String,
    body: String,
}

pub(super) fn escalated_max_tokens(current: u32) -> Option<u32> {
    let doubled = current.checked_mul(2)?;
    (doubled <= MAX_TOKENS_CEILING).then_some(doubled)
}

pub async fn generate(input: GroundingInput<'_>) -> Result<GroundingOutcome, String> {
    if input.api_key.is_empty() {
        return Err("agentic research requires an LLM API key".to_string());
    }
    if input.source_mode == GroundingSourceMode::Model {
        return Err("agentic research requires Web or My sources".to_string());
    }

    let (initial_queries, mut orchestration_usage) = plan_queries(&input).await?;
    let mut citations = Vec::new();
    let mut documents = Vec::new();
    let mut evidence = Vec::new();
    retrieve_queries(
        &input,
        &initial_queries,
        &mut evidence,
        &mut citations,
        &mut documents,
        &mut orchestration_usage,
    )
    .await?;
    if evidence.is_empty() {
        return Err("agentic research returned no evidence".to_string());
    }

    let (follow_up_queries, assessment_usage) = assess_gaps(&input, &evidence).await?;
    orchestration_usage = add_usage(&orchestration_usage, &assessment_usage);
    if !follow_up_queries.is_empty() {
        retrieve_queries(
            &input,
            &follow_up_queries,
            &mut evidence,
            &mut citations,
            &mut documents,
            &mut orchestration_usage,
        )
        .await?;
    }
    dedupe_strings(&mut citations);
    dedupe_documents(&mut documents);

    let final_prompt = format!(
        "{}\n\nAgentic research evidence gathered from these queries: {}\n\n{}\n\nUse the evidence above, resolve conflicts explicitly, and do not invent unsupported facts.",
        input.rendered_prompt,
        initial_queries
            .iter()
            .chain(follow_up_queries.iter())
            .map(|query| format!("\"{query}\""))
            .collect::<Vec<_>>()
            .join(", "),
        render_evidence(&evidence),
    );
    let final_input = GroundingInput {
        topic_name: input.topic_name,
        rendered_prompt: &final_prompt,
        compression_level: input.compression_level,
        model: input.model,
        api_key: input.api_key,
        api_base: input.api_base,
        reasoning: input.reasoning,
        existing_titles: input.existing_titles,
        documents: &documents,
        daily_card_count: input.daily_card_count,
        search: input.search.clone(),
        source_mode: input.source_mode,
        behavior: input.behavior,
        document_searcher: input.document_searcher,
    };
    let cards = generate_batch_with_retry(&final_input, BASE_MAX_TOKENS, false, true).await?;
    let Some(mut outcome) = GroundingOutcome::from_cards(cards, citations) else {
        return Err("agentic generation returned no cards".to_string());
    };
    outcome.usage = add_usage(&orchestration_usage, &outcome.usage);
    outcome.document_evidence = documents;
    Ok(outcome)
}

async fn plan_queries(input: &GroundingInput<'_>) -> Result<(Vec<String>, TokenUsage), String> {
    let prompt = format!(
        "Plan research for tip cards about \"{}\". Produce 2-3 complementary search queries that cover fundamentals, current details, and likely misconceptions. Respect this topic guidance: {}. Return JSON only: {{\"queries\":[\"...\"]}}",
        input.topic_name, input.search.profile.guidance,
    );
    let response = create_chat_completion_json(
        input.model,
        &prompt,
        input.api_key,
        input.api_base,
        input.reasoning,
        Some(1200),
    )
    .await;
    if response.is_error {
        return Err(format!(
            "agentic research planning failed: {}",
            response.content
        ));
    }
    let plan = parse_json_object::<QueryPlan>(&response.content)
        .ok_or_else(|| "agentic research planner returned invalid JSON".to_string())?;
    let queries = normalize_queries(plan.queries, INITIAL_QUERY_LIMIT);
    if queries.is_empty() {
        return Err("agentic research planner returned no queries".to_string());
    }
    Ok((queries, response.usage))
}

async fn assess_gaps(
    input: &GroundingInput<'_>,
    evidence: &[EvidenceNote],
) -> Result<(Vec<String>, TokenUsage), String> {
    let prompt = format!(
        "Review the research evidence below for tip cards about \"{}\". Identify material gaps, unresolved conflicts, or claims that need another search. Return zero to two follow-up queries as JSON only: {{\"missing_queries\":[\"...\"]}}. Return an empty array when coverage is sufficient.\n\n{}",
        input.topic_name,
        render_evidence(evidence),
    );
    let response = create_chat_completion_json(
        input.model,
        &prompt,
        input.api_key,
        input.api_base,
        input.reasoning,
        Some(1200),
    )
    .await;
    if response.is_error {
        return Err(format!(
            "agentic gap assessment failed: {}",
            response.content
        ));
    }
    let plan = parse_json_object::<GapPlan>(&response.content)
        .ok_or_else(|| "agentic gap assessment returned invalid JSON".to_string())?;
    Ok((
        normalize_queries(plan.missing_queries, FOLLOW_UP_QUERY_LIMIT),
        response.usage,
    ))
}

async fn retrieve_queries(
    input: &GroundingInput<'_>,
    queries: &[String],
    evidence: &mut Vec<EvidenceNote>,
    citations: &mut Vec<String>,
    documents: &mut Vec<DocChunk>,
    usage: &mut TokenUsage,
) -> Result<(), String> {
    for query in queries {
        if input.source_mode.uses_web() {
            if input.search.provider_native() {
                let response = create_chat_completion_grounded(
                    input.model,
                    &format!(
                        "Research this query for later card generation: {query}. Return a concise evidence memo. Call out disagreement or uncertainty between sources."
                    ),
                    input.api_key,
                    input.api_base,
                    input.reasoning,
                    Some(2400),
                    true,
                    false,
                )
                .await;
                if response.is_error {
                    return Err(format!("native web research failed: {}", response.content));
                }
                *usage = add_usage(usage, &response.usage);
                citations.extend(response.citations);
                evidence.push(EvidenceNote {
                    title: format!("Web research: {query}"),
                    body: response.content,
                });
            } else {
                let hits = search_external(&input.search, query).await?;
                citations.extend(hits.iter().map(|hit| hit.url.clone()));
                if !hits.is_empty() {
                    evidence.push(EvidenceNote {
                        title: format!("Web search: {query}"),
                        body: render_hits(&hits),
                    });
                }
            }
        }
        if input.source_mode.uses_documents() {
            let searcher = input
                .document_searcher
                .ok_or_else(|| "document research is not available".to_string())?;
            let found = searcher
                .search(query, input.search.profile.depth.result_limit())
                .await?;
            if !found.is_empty() {
                evidence.push(EvidenceNote {
                    title: format!("My sources: {query}"),
                    body: found
                        .iter()
                        .map(|document| format!("{}: {}", document.title, document.chunk))
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                });
                documents.extend(found);
            }
        }
        if evidence.len() >= EVIDENCE_LIMIT {
            break;
        }
    }
    Ok(())
}

fn normalize_queries(queries: Vec<String>, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    queries
        .into_iter()
        .map(|query| query.trim().chars().take(180).collect::<String>())
        .filter(|query| !query.is_empty())
        .filter(|query| seen.insert(query.to_ascii_lowercase()))
        .take(limit)
        .collect()
}

fn parse_json_object<T: for<'de> Deserialize<'de>>(content: &str) -> Option<T> {
    serde_json::from_str(content.trim()).ok().or_else(|| {
        let start = content.find('{')?;
        let end = content.rfind('}')?;
        serde_json::from_str(&content[start..=end]).ok()
    })
}

fn render_evidence(evidence: &[EvidenceNote]) -> String {
    evidence
        .iter()
        .take(EVIDENCE_LIMIT)
        .enumerate()
        .map(|(index, note)| {
            format!(
                "[E{}] {}\n{}",
                index + 1,
                note.title,
                note.body.chars().take(5000).collect::<String>()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.trim().to_ascii_lowercase()));
}

fn dedupe_documents(documents: &mut Vec<DocChunk>) {
    let mut seen = HashSet::new();
    documents.retain(|document| seen.insert((document.document_id, document.chunk.clone())));
}

#[cfg(test)]
mod tests {
    use super::{
        BASE_MAX_TOKENS, GapPlan, MAX_TOKENS_CEILING, QueryPlan, escalated_max_tokens,
        normalize_queries, parse_json_object,
    };

    #[test]
    fn planner_json_is_bounded_and_deduplicated() {
        let plan: QueryPlan = parse_json_object(
            "preface {\"queries\":[\" basics \",\"BASICS\",\"pitfalls\",\"current\"]} trailing",
        )
        .unwrap();
        assert_eq!(
            normalize_queries(plan.queries, 3),
            vec!["basics", "pitfalls", "current"]
        );
    }

    #[test]
    fn gap_plan_can_end_the_loop() {
        let plan: GapPlan = parse_json_object("{\"missing_queries\":[]}").unwrap();
        assert!(normalize_queries(plan.missing_queries, 2).is_empty());
    }

    #[test]
    fn escalation_doubles_until_the_ceiling() {
        assert_eq!(escalated_max_tokens(BASE_MAX_TOKENS), Some(16384));
        assert_eq!(escalated_max_tokens(16384), Some(32768));
        assert_eq!(escalated_max_tokens(MAX_TOKENS_CEILING), None);
    }
}
