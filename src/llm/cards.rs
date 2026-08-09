use crate::llm::{
    compression::CompressionLevel,
    markdown::{MarkdownSegment, join_markdown_segments, split_markdown_segments},
    transport::{
        LlmResponse, ReasoningConfig, TokenUsage, create_chat_completion,
        create_chat_completion_json,
    },
};

pub const DEFAULT_PROMPT_TEMPLATE: &str = "\
Write one genuinely useful daily tip about {topic}.

Make it practical, specific, and worth saving. Do not write a tiny fun fact.
Include:
- the core idea in plain language
- why it matters
- one concrete example, command, checklist, or mini workflow
- one caveat or common mistake when useful

Aim for 180-260 words. Markdown is allowed. Avoid filler, hype, and invented facts.";

const MIN_COMPRESS_CHARS: usize = 420;
const MIN_COMPRESS_WORDS: usize = 70;
pub const DEFAULT_COMPRESSION_LEVEL: &str = "strong";

pub(crate) const ONE_SHOT_FORMAT_INSTRUCTIONS: &str = "\
Return your response as a single JSON object with exactly these keys:
- \"title\": a concise, specific card title, maximum 8 words, no quotes, no markdown.
- \"content\": the full tip in markdown, practical and specific.
- \"compressed\": a compact, mobile-friendly version of the same tip in markdown.
- \"use_image\": true only when a visual materially improves understanding or retention (a diagram, spatial/physical identification, meaningful UI screenshot, or comparison).
- \"image_query\": a short, specific image-search query when \"use_image\" is true; otherwise an empty string.

Rules:
- Output ONLY valid JSON. Do not wrap it in markdown code fences.
- Do not add commentary outside the JSON object.
- Keep all facts, examples, commands, and caveats in the full \"content\".
- The \"compressed\" version must only shorten the prose; do not invent new claims.
- Do not request generic decoration, logos, repeated prose, or images that do not add information.";

pub(crate) const ARRAY_FORMAT_INSTRUCTIONS: &str = "\
Return your response as a single JSON object with exactly one top-level key, \"cards\".
The value of \"cards\" must be a JSON array of card objects, each with exactly these keys:
- \"title\": a concise, specific card title, maximum 8 words, no quotes, no markdown.
- \"content\": the full tip in markdown, practical and specific.
- \"compressed\": a compact, mobile-friendly version of the same tip in markdown.
- \"use_image\": true only when a visual materially improves understanding or retention (a diagram, spatial/physical identification, meaningful UI screenshot, or comparison).
- \"image_query\": a short, specific image-search query when \"use_image\" is true; otherwise an empty string.

Rules:
- Output ONLY the valid JSON object. Do not wrap it in markdown code fences.
- Do not add commentary outside the JSON object.
- Each card must cover a distinct, non-overlapping idea.
- Keep all facts, examples, commands, and caveats in each card's full \"content\".
- The \"compressed\" version must only shorten the prose; do not invent new claims.
- Do not request generic decoration, logos, repeated prose, or images that do not add information.";

#[derive(Debug, Clone)]
pub struct GeneratedCard {
    pub title: String,
    pub full_content: String,
    pub compressed_content: String,
    pub use_image: bool,
    pub image_query: String,
    pub usage: TokenUsage,
}

#[derive(Debug, serde::Deserialize, Default)]
pub(crate) struct ParsedGeneratedCard {
    pub(crate) title: Option<String>,
    pub(crate) content: Option<String>,
    pub(crate) compressed: Option<String>,
    #[serde(default)]
    pub(crate) use_image: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) image_query: Option<serde_json::Value>,
}

#[derive(Debug, serde::Deserialize)]
struct ParsedCardBatch {
    cards: Vec<ParsedGeneratedCard>,
}

/// How many times a single-card generation retries a request whose response was
/// an error or not parseable as the requested JSON shape.
const ONE_SHOT_MAX_ATTEMPTS: usize = 2;

pub async fn generate_card(
    rendered_prompt: &str,
    compression_level: CompressionLevel,
    model: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
) -> Result<GeneratedCard, String> {
    tracing::info!(
        model,
        prompt_len = rendered_prompt.len(),
        "LLM generate card one-shot"
    );
    if api_key.is_empty() {
        let fallback = format!("Generated tip (API KEY MISSING)\n\nPrompt:\n{rendered_prompt}");
        return Ok(GeneratedCard {
            title: fallback_title(&fallback),
            full_content: fallback.clone(),
            compressed_content: fallback,
            use_image: false,
            image_query: String::new(),
            usage: TokenUsage::default(),
        });
    }

    let prompt = format!(
        "{rendered_prompt}\n\n{ONE_SHOT_FORMAT_INSTRUCTIONS}\n\n\
         Compression target for the \"compressed\" field: {}.\n\n\
         Output ONLY valid JSON. Do not wrap in markdown code fences.",
        compression_level.oneshot_target()
    );

    let mut last_error = String::new();
    for attempt in 1..=ONE_SHOT_MAX_ATTEMPTS {
        let started = std::time::Instant::now();
        let response =
            create_chat_completion_json(model, &prompt, api_key, api_base, reasoning, Some(4096))
                .await;
        let duration_ms = started.elapsed().as_millis() as u64;
        if response.is_error {
            last_error = format!("one-shot generation failed: {}", response.content);
            tracing::warn!(
                model,
                attempt,
                duration_ms,
                error = %last_error,
                "LLM generate card one-shot failed"
            );
            if attempt < ONE_SHOT_MAX_ATTEMPTS {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                continue;
            }
            return Err(last_error);
        }

        let parsed = parse_generated_card_response(&response.content);
        if let Some(parsed) = parsed {
            return Ok(build_card_from_parsed(
                Some(parsed),
                &response.content,
                response.usage,
                compression_level,
                model,
                api_key,
                api_base,
            )
            .await);
        }
        last_error = format!(
            "model returned no parseable card JSON: {}",
            crate::llm::transport::content_snippet(&response.content, 200)
        );
        tracing::warn!(
            model,
            attempt,
            duration_ms,
            content_len = response.content.len(),
            error = %last_error,
            "one-shot card response was not parseable JSON"
        );
        if attempt < ONE_SHOT_MAX_ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
    }
    Err(last_error)
}

/// Finalize a parsed card into a [`GeneratedCard`]: resolve title/content with
/// fallbacks, fill the `compressed` field via the compression path when absent,
/// and accumulate compression token usage onto `base_usage`. When `parsed` is
/// `None` the raw response text becomes both full and compressed content.
/// Shared by [`generate_card`] and the grounding strategies.
pub(crate) async fn build_card_from_parsed(
    parsed: Option<ParsedGeneratedCard>,
    raw: &str,
    base_usage: TokenUsage,
    compression_level: CompressionLevel,
    model: &str,
    api_key: &str,
    api_base: &str,
) -> GeneratedCard {
    let (title, full_content, compressed_content, use_image, image_query, compress_usage) =
        if let Some(parsed) = parsed {
            let full_content = parsed
                .content
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| raw.trim().to_string());
            let title = parsed
                .title
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| fallback_title(&full_content));
            let (compressed_content, compress_usage) = if let Some(compressed) =
                parsed.compressed.filter(|value| !value.trim().is_empty())
            {
                (compressed, TokenUsage::default())
            } else if should_keep_full_card(&full_content) {
                (full_content.clone(), TokenUsage::default())
            } else {
                let compress_reasoning = ReasoningConfig::new(compression_level.reasoning_effort());
                let response = compress_card(
                    &full_content,
                    model,
                    api_key,
                    api_base,
                    compression_level,
                    &compress_reasoning,
                )
                .await;
                (response.content, response.usage)
            };
            let (use_image, image_query) =
                normalize_image_decision(parsed.use_image, parsed.image_query);
            (
                title,
                full_content,
                compressed_content,
                use_image,
                image_query,
                compress_usage,
            )
        } else {
            let full_content = raw.trim().to_string();
            let title = fallback_title(&full_content);
            (
                title,
                full_content.clone(),
                full_content,
                false,
                String::new(),
                TokenUsage::default(),
            )
        };

    let mut usage = base_usage;
    usage.prompt_tokens += compress_usage.prompt_tokens;
    usage.completion_tokens += compress_usage.completion_tokens;
    usage.total_tokens += compress_usage.total_tokens;

    GeneratedCard {
        title,
        full_content,
        compressed_content,
        use_image,
        image_query,
        usage,
    }
}

fn normalize_image_decision(
    use_image: Option<serde_json::Value>,
    image_query: Option<serde_json::Value>,
) -> (bool, String) {
    let use_image = use_image.and_then(|value| value.as_bool()).unwrap_or(false);
    let image_query = image_query
        .and_then(|value| value.as_str().map(str::trim).map(str::to_owned))
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(256).collect::<String>())
        .unwrap_or_default();
    if use_image && !image_query.is_empty() {
        (true, image_query)
    } else {
        (false, String::new())
    }
}

pub(crate) fn parse_generated_card_response(raw: &str) -> Option<ParsedGeneratedCard> {
    let cleaned = raw.trim();
    let json_text = strip_markdown_fences(cleaned).unwrap_or(cleaned);
    let json_text = json_text.trim();

    let parsed = if let Ok(parsed) = serde_json::from_str::<ParsedGeneratedCard>(json_text) {
        parsed
    } else if let Some(obj) = extract_json_object(json_text) {
        serde_json::from_str::<ParsedGeneratedCard>(obj).ok()?
    } else {
        return None;
    };

    // A JSON object with no usable content (for example a `{"cards": [...]}`
    // wrapper) is not a valid single card; treat it as unparseable so callers
    // retry or surface an error instead of storing a garbage card.
    if parsed
        .content
        .as_deref()
        .is_some_and(|content| !content.trim().is_empty())
    {
        Some(parsed)
    } else {
        None
    }
}

/// Parse the JSON-object batch contract `{ "cards": [...] }`. Legacy top-level
/// arrays remain accepted so responses from providers that ignore JSON-object
/// mode and older prompts continue to work. Markdown fences and surrounding
/// prose are tolerated for both shapes.
pub(crate) fn parse_card_array(raw: &str) -> Vec<ParsedGeneratedCard> {
    let cleaned = raw.trim();
    let json_text = strip_markdown_fences(cleaned).unwrap_or(cleaned);
    let json_text = json_text.trim();

    if let Some(cards) = parse_card_batch_json(json_text) {
        return cards;
    }

    if let Some(object) = extract_json_object(json_text) {
        if let Some(cards) = parse_card_batch_json(object) {
            return cards;
        }
    }

    if let Some(arr) = extract_json_array(json_text) {
        if let Some(cards) = parse_card_batch_json(arr) {
            return cards;
        }
    }

    Vec::new()
}

fn parse_card_batch_json(json_text: &str) -> Option<Vec<ParsedGeneratedCard>> {
    if let Ok(batch) = serde_json::from_str::<ParsedCardBatch>(json_text) {
        return Some(valid_generated_cards(batch.cards));
    }
    serde_json::from_str::<Vec<ParsedGeneratedCard>>(json_text)
        .ok()
        .map(valid_generated_cards)
}

fn valid_generated_cards(cards: Vec<ParsedGeneratedCard>) -> Vec<ParsedGeneratedCard> {
    cards
        .into_iter()
        .filter(|card| {
            card.content
                .as_deref()
                .is_some_and(|content| !content.trim().is_empty())
        })
        .collect()
}

/// Extract the first balanced top-level `[...]` array span, skipping brackets
/// that appear inside JSON string literals. Mirrors [`extract_json_object`].
fn extract_json_array(raw: &str) -> Option<&str> {
    let start = raw.find('[')?;
    let bytes = raw.as_bytes();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for i in start..raw.len() {
        let c = bytes[i] as char;
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
        } else {
            match c {
                '"' => in_string = true,
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&raw[start..=i]);
                    }
                }
                _ => {}
            }
        }
    }

    None
}

fn strip_markdown_fences(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return None;
    }
    let start = trimmed.find('\n')? + 1;
    let end = trimmed.rfind("```")?;
    if end <= start {
        return None;
    }
    Some(trimmed[start..end].trim())
}

fn extract_json_object(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let bytes = raw.as_bytes();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for i in start..raw.len() {
        let c = bytes[i] as char;
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
        } else {
            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&raw[start..=i]);
                    }
                }
                _ => {}
            }
        }
    }

    None
}

async fn compress_text_segment(
    text: &str,
    model: &str,
    api_key: &str,
    api_base: &str,
    level: CompressionLevel,
    reasoning: &ReasoningConfig,
) -> LlmResponse {
    let trimmed = text.trim();
    if trimmed.is_empty() || should_keep_full_card(trimmed) {
        return LlmResponse {
            content: trimmed.to_string(),
            usage: TokenUsage::default(),
            citations: Vec::new(),
            is_error: false,
        };
    }

    if api_key.is_empty() {
        return LlmResponse {
            content: format!("Compressed: {}", trimmed),
            usage: TokenUsage::default(),
            citations: Vec::new(),
            is_error: false,
        };
    }

    let prompt = format!(
        "Create the compact card text for this tip excerpt.\n\
         Rules:\n\
         - Use only facts, steps, commands, examples, and caveats from the source tip.\n\
         - Do not add new claims, numbers, tools, citations, links, or explanations.\n\
         - Do not invent fenced code blocks.\n\
         - If the source is already card-sized, return it unchanged.\n\
         - Return only the compact card text.\n\n\
          {}\n\n\
          Source tip excerpt:\n{}",
        level.prompt_rules(),
        trimmed
    );
    create_chat_completion(model, &prompt, api_key, api_base, reasoning, Some(1024)).await
}

pub async fn compress_card(
    full_content: &str,
    model: &str,
    api_key: &str,
    api_base: &str,
    level: CompressionLevel,
    reasoning: &ReasoningConfig,
) -> LlmResponse {
    tracing::info!(
        model,
        ?level,
        content_len = full_content.len(),
        "LLM compress card"
    );
    if should_keep_full_card(full_content) {
        return LlmResponse {
            content: full_content.trim().to_string(),
            usage: TokenUsage::default(),
            citations: Vec::new(),
            is_error: false,
        };
    }

    let segments = split_markdown_segments(full_content);
    if segments.is_empty() {
        return LlmResponse {
            content: full_content.trim().to_string(),
            usage: TokenUsage::default(),
            citations: Vec::new(),
            is_error: false,
        };
    }

    let mut code_entries = Vec::new();
    let mut tasks = Vec::new();

    for (idx, segment) in segments.into_iter().enumerate() {
        match segment {
            MarkdownSegment::Code(code) => {
                code_entries.push((idx, MarkdownSegment::Code(code)));
            }
            MarkdownSegment::Text(text) => {
                let model = model.to_string();
                let api_key = api_key.to_string();
                let api_base = api_base.to_string();
                let reasoning = reasoning.clone();
                tasks.push(tokio::spawn(async move {
                    let response = compress_text_segment(
                        &text, &model, &api_key, &api_base, level, &reasoning,
                    )
                    .await;
                    (idx, MarkdownSegment::Text(response.content), response.usage)
                }));
            }
        }
    }

    let mut results = code_entries;
    let mut total_usage = TokenUsage::default();
    for task in tasks {
        match task.await {
            Ok((idx, segment, usage)) => {
                total_usage.prompt_tokens += usage.prompt_tokens;
                total_usage.completion_tokens += usage.completion_tokens;
                total_usage.total_tokens += usage.total_tokens;
                results.push((idx, segment));
            }
            Err(err) => {
                tracing::error!(%err, "Compression segment task panicked");
                return LlmResponse::error(format!(
                    "LLM Error: compression segment task panicked: {err}"
                ));
            }
        }
    }

    results.sort_by_key(|(idx, _)| *idx);
    let compressed_segments: Vec<_> = results.into_iter().map(|(_, segment)| segment).collect();

    LlmResponse {
        content: join_markdown_segments(&compressed_segments),
        usage: total_usage,
        citations: Vec::new(),
        is_error: false,
    }
}

fn should_keep_full_card(full_content: &str) -> bool {
    let trimmed = full_content.trim();
    if trimmed.is_empty() {
        return true;
    }

    let chars = trimmed.chars().count();
    let words = trimmed.split_whitespace().count();
    chars <= MIN_COMPRESS_CHARS || words <= MIN_COMPRESS_WORDS
}

fn fallback_title(full_content: &str) -> String {
    let first_line = full_content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Untitled card")
        .trim_start_matches('#')
        .trim();

    let title = first_line
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join(" ");

    if title.is_empty() {
        "Untitled card".to_string()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ParsedGeneratedCard, extract_json_object, fallback_title, normalize_image_decision,
        parse_card_array, parse_generated_card_response, should_keep_full_card,
        strip_markdown_fences,
    };

    #[test]
    fn fallback_title_uses_first_non_empty_line_without_heading_marker() {
        assert_eq!(
            fallback_title("\n## Borrow Checking Prevents Aliasing Bugs\nMore text"),
            "Borrow Checking Prevents Aliasing Bugs"
        );
    }

    #[test]
    fn fallback_title_limits_word_count() {
        assert_eq!(
            fallback_title("one two three four five six seven eight nine ten"),
            "one two three four five six seven eight"
        );
    }

    #[test]
    fn small_card_is_not_compressed() {
        let card = "Use `cargo check` before a full test run. It catches type errors quickly and keeps the edit loop short.";
        assert!(should_keep_full_card(card));
    }

    #[test]
    fn long_card_is_compressed() {
        let card = (0..90)
            .map(|_| "Keep the actionable detail that matters for the reader.")
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!should_keep_full_card(&card));
    }

    #[test]
    fn strip_markdown_fences_extracts_inner_json() {
        let raw = "```json\n{\"title\":\"T\",\"content\":\"C\"}\n```";
        assert_eq!(
            strip_markdown_fences(raw),
            Some("{\"title\":\"T\",\"content\":\"C\"}")
        );
    }

    #[test]
    fn strip_markdown_fences_returns_none_for_plain_text() {
        assert_eq!(strip_markdown_fences("plain text"), None);
    }

    #[test]
    fn extract_json_object_skips_preamble_and_balances_braces() {
        let raw = "Here is the JSON:\n{\"title\":\"A\",\"nested\":{\"x\":1}}\nDone.";
        assert_eq!(
            extract_json_object(raw),
            Some("{\"title\":\"A\",\"nested\":{\"x\":1}}")
        );
    }

    #[test]
    fn extract_json_object_ignores_braces_in_strings() {
        let raw = r#"{"title":"A {brace}","content":"C"}"#;
        assert_eq!(
            extract_json_object(raw),
            Some(r#"{"title":"A {brace}","content":"C"}"#)
        );
    }

    #[test]
    fn parse_generated_card_response_reads_plain_json() {
        let parsed = parse_generated_card_response(
            r#"{"title":"Test Title","content":"Full body","compressed":"Short"}"#,
        )
        .unwrap();
        assert_eq!(parsed.title, Some("Test Title".to_string()));
        assert_eq!(parsed.content, Some("Full body".to_string()));
        assert_eq!(parsed.compressed, Some("Short".to_string()));
    }

    #[test]
    fn parse_generated_card_response_reads_fenced_json() {
        let parsed = parse_generated_card_response(
            "```json\n{\"title\":\"T\",\"content\":\"C\",\"compressed\":\"S\"}\n```",
        )
        .unwrap();
        assert_eq!(parsed.title, Some("T".to_string()));
        assert_eq!(parsed.content, Some("C".to_string()));
        assert_eq!(parsed.compressed, Some("S".to_string()));
    }

    #[test]
    fn parse_generated_card_response_extracts_object_from_extra_text() {
        let parsed = parse_generated_card_response(
            "Sure!\n{\"title\":\"T\",\"content\":\"C\",\"compressed\":\"S\"}\nHope this helps!",
        )
        .unwrap();
        assert_eq!(parsed.title, Some("T".to_string()));
        assert_eq!(parsed.content, Some("C".to_string()));
        assert_eq!(parsed.compressed, Some("S".to_string()));
    }

    #[test]
    fn parse_generated_card_response_returns_none_for_invalid_input() {
        assert!(parse_generated_card_response("not json").is_none());
    }

    #[test]
    fn parsed_generated_card_defaults_missing_fields() {
        let parsed: ParsedGeneratedCard =
            serde_json::from_str(r#"{"title":"Only title"}"#).unwrap();
        assert_eq!(parsed.title, Some("Only title".to_string()));
        assert_eq!(parsed.content, None);
        assert_eq!(parsed.compressed, None);
    }

    #[test]
    fn card_array_discards_entries_without_content() {
        let cards = parse_card_array(
            r#"[{"title":"Empty"},{"title":"Blank","content":"  "},{"title":"Useful","content":"Body","use_image":true,"image_query":"diagram"}]"#,
        );
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title.as_deref(), Some("Useful"));
    }

    #[test]
    fn card_array_reads_json_object_batch() {
        let cards = parse_card_array(
            r#"{"cards":[{"title":"First","content":"Body 1"},{"title":"Second","content":"Body 2"}]}"#,
        );
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].title.as_deref(), Some("First"));
        assert_eq!(cards[1].title.as_deref(), Some("Second"));
    }

    #[test]
    fn card_array_reads_fenced_json_object_batch() {
        let cards = parse_card_array(
            "```json\n{\"cards\":[{\"title\":\"First\",\"content\":\"Body\"}]}\n```",
        );
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title.as_deref(), Some("First"));
    }

    #[test]
    fn card_array_extracts_json_object_batch_from_extra_text() {
        let cards = parse_card_array(
            "Batch follows:\n{\"cards\":[{\"title\":\"First\",\"content\":\"Body\"}]}\nDone.",
        );
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title.as_deref(), Some("First"));
    }

    #[test]
    fn card_array_rejects_lone_card_object() {
        let cards = parse_card_array(r#"{"title":"Only","content":"Body"}"#);
        assert!(cards.is_empty());
    }

    #[test]
    fn image_decision_requires_boolean_and_nonblank_query() {
        assert_eq!(
            normalize_image_decision(
                Some(serde_json::json!(true)),
                Some(serde_json::json!("  diagram  "))
            ),
            (true, "diagram".to_string())
        );
        assert_eq!(
            normalize_image_decision(Some(serde_json::json!(true)), Some(serde_json::json!("  "))),
            (false, String::new())
        );
        assert_eq!(
            normalize_image_decision(
                Some(serde_json::json!("true")),
                Some(serde_json::json!("diagram"))
            ),
            (false, String::new())
        );
    }

    #[test]
    fn image_decision_trims_and_caps_query() {
        let query = "x".repeat(300);
        let (_, normalized) = normalize_image_decision(
            Some(serde_json::json!(true)),
            Some(serde_json::json!(format!(" {query} "))),
        );
        assert_eq!(normalized.chars().count(), 256);
    }
}
