use crate::context::CardContext;
use crate::domain::grounding::GroundingStrategy;
use crate::llm::cards::DEFAULT_PROMPT_TEMPLATE;
use crate::llm::transport::{LlmResponse, ReasoningConfig, create_chat_completion_json};

const MAX_PROMPT_CHARS: usize = 4000;
const MAX_RATIONALE_CHARS: usize = 500;

const GROUNDING_STRATEGIES: &[&str] = &["factual", "create_and_ground", "agentic", "rag"];
const IMAGE_STRATEGIES: &[&str] = &[
    "none",
    "pool",
    "bing_html",
    "bing_playwright",
    "ddgs_text_og",
];
const REASONING_EFFORTS: &[&str] = &["none", "minimal", "low", "medium", "high", "xhigh"];

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PromptEnhanceSuggestion {
    pub prompt_template: String,
    pub grounding_strategy: String,
    pub grounding_model: String,
    pub grounding_reasoning_effort: String,
    pub image_strategy: String,
    pub rationale: String,
}

#[derive(Clone, Debug)]
pub struct PromptEnhanceInput<'a> {
    pub topic_name: &'a str,
    pub current_prompt: &'a str,
    pub current_grounding_strategy: &'a str,
    pub current_grounding_model: &'a str,
    pub current_grounding_reasoning_effort: &'a str,
    pub current_image_strategy: &'a str,
    pub history: &'a CardContext,
    pub has_documents: bool,
}

#[derive(Debug, serde::Deserialize, Default)]
struct ParsedSuggestion {
    prompt_template: Option<String>,
    grounding_strategy: Option<String>,
    grounding_model: Option<String>,
    grounding_reasoning_effort: Option<String>,
    image_strategy: Option<String>,
    rationale: Option<String>,
}

pub async fn suggest_prompt_template(
    input: &PromptEnhanceInput<'_>,
    model: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
) -> LlmResponse {
    tracing::info!(
        topic = input.topic_name,
        model,
        titles = input.history.existing_titles().len(),
        "LLM suggest prompt template"
    );
    if api_key.is_empty() {
        let fallback = history_fallback(input);
        return LlmResponse {
            content: encode_suggestion(&fallback),
            usage: crate::llm::TokenUsage::default(),
            citations: Vec::new(),
            is_error: false,
            truncated: false,
        };
    }

    let prompt = assemble_enhance_prompt(input);
    let response =
        create_chat_completion_json(model, &prompt, api_key, api_base, reasoning, Some(2048)).await;
    if response.is_error {
        tracing::warn!(
            topic = input.topic_name,
            error = %response.content,
            "prompt template suggestion failed; using history fallback"
        );
        let fallback = history_fallback(input);
        return LlmResponse {
            content: encode_suggestion(&fallback),
            usage: response.usage,
            citations: Vec::new(),
            is_error: false,
            truncated: false,
        };
    }

    let parsed = parse_suggestion(&response.content).unwrap_or_default();
    let suggestion = sanitize_suggestion(input, parsed);
    LlmResponse {
        content: encode_suggestion(&suggestion),
        usage: response.usage,
        citations: Vec::new(),
        is_error: false,
        truncated: false,
    }
}

pub fn decode_suggestion(content: &str) -> Option<PromptEnhanceSuggestion> {
    serde_json::from_str(content).ok()
}

pub(crate) fn assemble_enhance_prompt(input: &PromptEnhanceInput<'_>) -> String {
    let history = input.history.render_all();
    let history = if history.is_empty() {
        "No generated cards yet.".to_string()
    } else {
        history
    };
    let current_prompt = if input.current_prompt.trim().is_empty() {
        DEFAULT_PROMPT_TEMPLATE
    } else {
        input.current_prompt
    };
    let topic = if input.topic_name.trim().is_empty() {
        "(global default across topics)"
    } else {
        input.topic_name
    };
    let documents = if input.has_documents { "yes" } else { "no" };

    format!(
        "You rewrite Denpie card-generation prompt templates.\n\
         Denpie writes daily learning cards. The template MUST contain {{topic}}.\n\
         The server already appends existing card titles with [known]/[hard]/[skip] labels.\n\
         Do not restate those labels. Do not ask for a fixed word count or \"write one card\".\n\
         Keep the template batch-agnostic so agentic/RAG wrappers can add their own brief.\n\
         Prefer specific, practical cards. Avoid filler and invented facts.\n\n\
         Topic: {topic}\n\
         Current prompt template:\n{current_prompt}\n\n\
         Current grounding:\n\
         strategy={strategy}\n\
         model={model}\n\
         reasoning={reasoning}\n\
         image_strategy={image}\n\
         Documents assigned: {documents}\n\n\
         Generated-card history:\n{history}\n\n\
         Return JSON only with these keys:\n\
         {{\"prompt_template\":\"...\",\"grounding_strategy\":\"factual|create_and_ground|agentic|rag or empty\",\
         \"grounding_model\":\"empty to keep\",\"grounding_reasoning_effort\":\"none|minimal|low|medium|high|xhigh or empty\",\
         \"image_strategy\":\"none|pool|bing_html|bing_playwright|ddgs_text_og or empty\",\
         \"rationale\":\"short explanation citing the history\"}}\n\
         Empty grounding fields mean keep the current value.\n\
         If documents are assigned and the cards need source-backed claims, prefer rag.\n\
         If titles look shallow or repetitive, prefer agentic.\n\
         If many cards are [hard], ask for smaller prerequisites.\n\
         If many cards are [known], ask for the next layer of difficulty.\n\
         If many cards are [skip], steer away from those angles.",
        strategy = display_setting(input.current_grounding_strategy),
        model = display_setting(input.current_grounding_model),
        reasoning = display_setting(input.current_grounding_reasoning_effort),
        image = display_setting(input.current_image_strategy),
    )
}

pub(crate) fn history_fallback(input: &PromptEnhanceInput<'_>) -> PromptEnhanceSuggestion {
    let base = if input.current_prompt.trim().is_empty() {
        DEFAULT_PROMPT_TEMPLATE.trim_end()
    } else {
        input.current_prompt.trim_end()
    };
    let known = input.history.known_count();
    let hard = input.history.hard_count();
    let skip = input.history.skip_count();
    let titles = input.history.existing_titles().len();

    let mut extras = Vec::new();
    if hard > 0 && hard >= known {
        extras.push(
            "When earlier cards were too hard, start from a smaller prerequisite and one concrete example.",
        );
    } else if known > 0 && known >= hard {
        extras
            .push("When earlier cards were already known, go one layer deeper than those titles.");
    }
    if skip > 0 {
        extras.push("Do not cover skipped angles or similar framing.");
    }
    if titles == 0 {
        extras.push("Cover a useful first slice of the topic, not a survey.");
    }

    let mut prompt = base.to_string();
    if !extras.is_empty() && extras.iter().any(|line| !prompt.contains(line)) {
        prompt.push('\n');
        for line in extras {
            if !prompt.contains(line) {
                prompt.push('\n');
                prompt.push_str(line);
            }
        }
    }
    if !prompt.contains("{topic}") {
        prompt = DEFAULT_PROMPT_TEMPLATE.to_string();
    }

    let mut grounding_strategy = String::new();
    let mut rationale_bits = Vec::new();
    if input.has_documents && input.current_grounding_strategy != GroundingStrategy::Rag.as_str() {
        grounding_strategy = GroundingStrategy::Rag.as_str().to_string();
        rationale_bits.push("assigned documents, so grounding should read those sources");
    } else if titles >= 8
        && skip + hard >= 2
        && input.current_grounding_strategy != GroundingStrategy::Agentic.as_str()
        && !input.has_documents
    {
        grounding_strategy = GroundingStrategy::Agentic.as_str().to_string();
        rationale_bits.push("enough review signal to research a backlog instead of one-off cards");
    }

    if known > 0 {
        rationale_bits.push("known cards should not be repeated");
    }
    if hard > 0 {
        rationale_bits.push("hard cards need easier on-ramps");
    }
    if skip > 0 {
        rationale_bits.push("skipped cards mark angles to avoid");
    }
    if rationale_bits.is_empty() {
        rationale_bits.push("no review labels yet; kept the template close to the current one");
    }

    PromptEnhanceSuggestion {
        prompt_template: prompt,
        grounding_strategy,
        grounding_model: String::new(),
        grounding_reasoning_effort: String::new(),
        image_strategy: String::new(),
        rationale: format!(
            "Looked at {titles} recent titles ({known} known, {hard} hard, {skip} skip): {}.",
            rationale_bits.join("; ")
        ),
    }
}

fn sanitize_suggestion(
    input: &PromptEnhanceInput<'_>,
    parsed: ParsedSuggestion,
) -> PromptEnhanceSuggestion {
    let mut prompt = parsed
        .prompt_template
        .unwrap_or_default()
        .trim()
        .to_string();
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        prompt = prompt.chars().take(MAX_PROMPT_CHARS).collect();
    }
    if !prompt.contains("{topic}") {
        prompt = if input.current_prompt.contains("{topic}") {
            input.current_prompt.to_string()
        } else {
            DEFAULT_PROMPT_TEMPLATE.to_string()
        };
    }

    let grounding_strategy =
        allowlisted(parsed.grounding_strategy.as_deref(), GROUNDING_STRATEGIES);
    let image_strategy = allowlisted(parsed.image_strategy.as_deref(), IMAGE_STRATEGIES);
    let grounding_reasoning_effort = allowlisted(
        parsed.grounding_reasoning_effort.as_deref(),
        REASONING_EFFORTS,
    );
    let grounding_model = parsed
        .grounding_model
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut rationale = parsed.rationale.unwrap_or_default().trim().to_string();
    if rationale.chars().count() > MAX_RATIONALE_CHARS {
        rationale = rationale.chars().take(MAX_RATIONALE_CHARS).collect();
    }
    if rationale.is_empty() {
        rationale = "Suggested from generated-card history.".to_string();
    }

    PromptEnhanceSuggestion {
        prompt_template: prompt,
        grounding_strategy,
        grounding_model,
        grounding_reasoning_effort,
        image_strategy,
        rationale,
    }
}

fn parse_suggestion(content: &str) -> Option<ParsedSuggestion> {
    let trimmed = content.trim();
    let json_text = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    if let Ok(parsed) = serde_json::from_str::<ParsedSuggestion>(json_text) {
        return Some(parsed);
    }
    let start = json_text.find('{')?;
    let end = json_text.rfind('}')?;
    serde_json::from_str(&json_text[start..=end]).ok()
}

fn encode_suggestion(suggestion: &PromptEnhanceSuggestion) -> String {
    serde_json::to_string(suggestion).unwrap_or_else(|_| {
        serde_json::json!({
            "prompt_template": DEFAULT_PROMPT_TEMPLATE,
            "grounding_strategy": "",
            "grounding_model": "",
            "grounding_reasoning_effort": "",
            "image_strategy": "",
            "rationale": "Could not encode suggestion.",
        })
        .to_string()
    })
}

fn allowlisted(value: Option<&str>, allowed: &[&str]) -> String {
    let trimmed = value.unwrap_or("").trim();
    if allowed.contains(&trimmed) {
        trimmed.to_string()
    } else {
        String::new()
    }
}

fn display_setting(value: &str) -> &str {
    if value.trim().is_empty() {
        "(inherit)"
    } else {
        value.trim()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(
        prompt: &'a str,
        history: &'a CardContext,
        has_documents: bool,
    ) -> PromptEnhanceInput<'a> {
        PromptEnhanceInput {
            topic_name: "Rust",
            current_prompt: prompt,
            current_grounding_strategy: "factual",
            current_grounding_model: "",
            current_grounding_reasoning_effort: "",
            current_image_strategy: "none",
            history,
            has_documents,
        }
    }

    #[test]
    fn assembled_prompt_includes_history_and_forbids_one_shot_briefs() {
        let history = CardContext::from_parts(
            vec!["Borrow checker basics".to_string()],
            Vec::new(),
            vec!["Borrow checker basics".to_string()],
            Vec::new(),
            Vec::new(),
        );
        let prompt = assemble_enhance_prompt(&input(DEFAULT_PROMPT_TEMPLATE, &history, false));
        assert!(prompt.contains("{topic}"));
        assert!(prompt.contains("Borrow checker basics"));
        assert!(prompt.contains("[known]"));
        assert!(prompt.contains("batch-agnostic"));
        assert!(!prompt.contains("180-260"));
    }

    #[test]
    fn sanitize_rejects_templates_without_topic_placeholder() {
        let history = CardContext::default();
        let suggestion = sanitize_suggestion(
            &input(DEFAULT_PROMPT_TEMPLATE, &history, false),
            ParsedSuggestion {
                prompt_template: Some("Write a poem.".into()),
                rationale: Some("style".into()),
                ..ParsedSuggestion::default()
            },
        );
        assert!(suggestion.prompt_template.contains("{topic}"));
        assert_eq!(suggestion.prompt_template, DEFAULT_PROMPT_TEMPLATE);
    }

    #[test]
    fn sanitize_drops_unknown_grounding_values() {
        let history = CardContext::default();
        let suggestion = sanitize_suggestion(
            &input(DEFAULT_PROMPT_TEMPLATE, &history, false),
            ParsedSuggestion {
                prompt_template: Some("Write useful daily tip cards about {topic}.".into()),
                grounding_strategy: Some("magic".into()),
                image_strategy: Some("telepathy".into()),
                grounding_reasoning_effort: Some("ludicrous".into()),
                ..ParsedSuggestion::default()
            },
        );
        assert!(suggestion.grounding_strategy.is_empty());
        assert!(suggestion.image_strategy.is_empty());
        assert!(suggestion.grounding_reasoning_effort.is_empty());
    }

    #[test]
    fn history_fallback_asks_for_prerequisites_when_cards_were_hard() {
        let history = CardContext::from_parts(
            vec!["Lifetime annotations".to_string()],
            Vec::new(),
            Vec::new(),
            vec!["Lifetime annotations".to_string()],
            Vec::new(),
        );
        let suggestion = history_fallback(&input("", &history, false));
        assert!(suggestion.prompt_template.contains("{topic}"));
        assert!(suggestion.prompt_template.contains("prerequisite"));
        assert!(suggestion.rationale.contains("hard"));
    }

    #[test]
    fn history_fallback_picks_rag_when_documents_are_assigned() {
        let history = CardContext::default();
        let suggestion = history_fallback(&input(DEFAULT_PROMPT_TEMPLATE, &history, true));
        assert_eq!(
            suggestion.grounding_strategy,
            GroundingStrategy::Rag.as_str()
        );
        assert!(suggestion.rationale.contains("documents"));
    }

    #[test]
    fn parse_suggestion_reads_fenced_json() {
        let parsed = parse_suggestion(
            "```json\n{\"prompt_template\":\"Write about {topic}.\",\"rationale\":\"ok\"}\n```",
        )
        .unwrap();
        assert_eq!(
            parsed.prompt_template.as_deref(),
            Some("Write about {topic}.")
        );
        assert_eq!(parsed.rationale.as_deref(), Some("ok"));
    }
}
