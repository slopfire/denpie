use crate::llm::{
    compression::CompressionLevel,
    markdown::{MarkdownSegment, join_markdown_segments, split_markdown_segments},
    transport::{LlmResponse, ReasoningConfig, TokenUsage, create_chat_completion},
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

pub async fn generate_new_card(
    model: &str,
    prompt: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
) -> LlmResponse {
    tracing::info!(model, prompt_len = prompt.len(), "LLM generate new card");
    if api_key.is_empty() {
        return LlmResponse {
            content: format!("Generated tip (API KEY MISSING)\n\nPrompt:\n{}", prompt),
            usage: TokenUsage::default(),
        };
    }

    create_chat_completion(model, prompt, api_key, api_base, reasoning).await
}

pub async fn generate_card_title(
    full_content: &str,
    model: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
) -> LlmResponse {
    tracing::info!(model, "LLM generate card title");
    if api_key.is_empty() {
        return LlmResponse {
            content: fallback_title(full_content),
            usage: TokenUsage::default(),
        };
    }

    let prompt = format!(
        "Generate one concise card title for this tip.\n\
         Requirements:\n\
         - Return only the title, no quotes and no markdown.\n\
         - Maximum 8 words.\n\
         - Make it specific enough to distinguish the card from related tips.\n\n\
         Tip:\n{}",
        full_content
    );
    create_chat_completion(model, &prompt, api_key, api_base, reasoning).await
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
        };
    }

    if api_key.is_empty() {
        return LlmResponse {
            content: format!("Compressed: {}", trimmed),
            usage: TokenUsage::default(),
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
    create_chat_completion(model, &prompt, api_key, api_base, reasoning).await
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
        };
    }

    let segments = split_markdown_segments(full_content);
    if segments.is_empty() {
        return LlmResponse {
            content: full_content.trim().to_string(),
            usage: TokenUsage::default(),
        };
    }

    let mut compressed_segments = Vec::with_capacity(segments.len());
    let mut total_usage = TokenUsage::default();

    for segment in segments {
        match segment {
            MarkdownSegment::Code(code) => compressed_segments.push(MarkdownSegment::Code(code)),
            MarkdownSegment::Text(text) => {
                let response =
                    compress_text_segment(&text, model, api_key, api_base, level, reasoning).await;
                total_usage.prompt_tokens += response.usage.prompt_tokens;
                total_usage.completion_tokens += response.usage.completion_tokens;
                total_usage.total_tokens += response.usage.total_tokens;
                compressed_segments.push(MarkdownSegment::Text(response.content));
            }
        }
    }

    LlmResponse {
        content: join_markdown_segments(&compressed_segments),
        usage: total_usage,
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
    use super::{fallback_title, should_keep_full_card};

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
}
