use serde_json::{json, Value};

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
pub const DEFAULT_COMPRESSION_LEVEL: &str = "balanced";

#[derive(Clone, Debug, Default)]
pub struct TokenUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Clone, Debug)]
pub struct LlmResponse {
    pub content: String,
    pub usage: TokenUsage,
}

#[derive(Clone, Debug)]
pub struct ReasoningConfig {
    pub effort: String,
}

impl ReasoningConfig {
    pub fn new(effort: impl Into<String>) -> Self {
        Self {
            effort: effort.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionLevel {
    Light,
    Balanced,
    Strong,
    Ultra,
}

impl CompressionLevel {
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "strong" => Self::Strong,
            "ultra" => Self::Ultra,
            _ => Self::Balanced,
        }
    }

    pub fn as_setting(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Balanced => "balanced",
            Self::Strong => "strong",
            Self::Ultra => "ultra",
        }
    }

    pub fn reasoning_effort(self) -> &'static str {
        match self {
            Self::Light => "minimal",
            Self::Balanced => "low",
            Self::Strong => "medium",
            Self::Ultra => "high",
        }
    }

    fn prompt_rules(self) -> &'static str {
        match self {
            Self::Light => {
                "Preset: Light compression.\n\
                 - Preserve nearly all useful detail, examples, and caveats.\n\
                 - Target 110-150 words, or about 650-900 characters.\n\
                 - Prefer trimming connective prose over removing steps."
            }
            Self::Balanced => {
                "Preset: Balanced compression.\n\
                 - Preserve the most useful actionable details; never reduce it to a vague teaser.\n\
                 - Target 70-110 words, or about 420-650 characters.\n\
                 - Keep markdown if it improves scanning on mobile."
            }
            Self::Strong => {
                "Preset: Strong compression.\n\
                 - Keep only the core idea, the highest-value example or command, and one caveat when important.\n\
                 - Target 40-70 words, or about 250-420 characters.\n\
                 - Use tight bullets or compact sentences."
            }
            Self::Ultra => {
                "Preset: Ultra compression.\n\
                 - Return a reminder-sized card with the action, trigger, and critical caveat only.\n\
                 - Target 18-35 words, or about 120-240 characters.\n\
                 - Avoid setup, explanation, and nice-to-have context."
            }
        }
    }
}

pub async fn generate_new_card(
    model: &str,
    prompt: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
) -> LlmResponse {
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

pub async fn compress_card(
    full_content: &str,
    model: &str,
    api_key: &str,
    api_base: &str,
    level: CompressionLevel,
    reasoning: &ReasoningConfig,
) -> LlmResponse {
    if should_keep_full_card(full_content) {
        return LlmResponse {
            content: full_content.trim().to_string(),
            usage: TokenUsage::default(),
        };
    }

    if api_key.is_empty() {
        return LlmResponse {
            content: format!("Compressed: {}", full_content),
            usage: TokenUsage::default(),
        };
    }

    let prompt = format!(
        "Create the compact card text for this tip.\n\
         Rules:\n\
         - Use only facts, steps, commands, examples, and caveats from the source tip.\n\
         - Do not add new claims, numbers, tools, citations, links, or explanations.\n\
         - If the source is already card-sized, return it unchanged.\n\
         - Return only the compact card text.\n\n\
         {}\n\n\
         Source tip:\n{}",
        level.prompt_rules(),
        full_content
    );
    create_chat_completion(model, &prompt, api_key, api_base, reasoning).await
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

async fn create_chat_completion(
    model: &str,
    prompt: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
) -> LlmResponse {
    let effort = normalize_reasoning_effort(&reasoning.effort);
    let req = json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": prompt
            }
        ],
        "reasoning": {
            "effort": effort
        }
    });

    let url = format!("{}/chat/completions", api_base.trim_end_matches('/'));
    let client = reqwest::Client::new();

    match client
        .post(url)
        .bearer_auth(api_key)
        .json(&req)
        .send()
        .await
    {
        Ok(res) => {
            if !res.status().is_success() {
                let status = res.status();
                let body = res.text().await.unwrap_or_default();
                return LlmResponse {
                    content: format!("LLM Error: HTTP {} {}", status, body),
                    usage: TokenUsage::default(),
                };
            }

            match res.json::<Value>().await {
                Ok(body) => {
                    let content = body["choices"]
                        .as_array()
                        .and_then(|choices| choices.first())
                        .and_then(|choice| choice["message"]["content"].as_str())
                        .map(str::to_string)
                        .unwrap_or("Failed parsing text".to_string());
                    LlmResponse {
                        content,
                        usage: parse_token_usage(&body),
                    }
                }
                Err(e) => LlmResponse {
                    content: format!("LLM Error: {}", e),
                    usage: TokenUsage::default(),
                },
            }
        }
        Err(e) => LlmResponse {
            content: format!("LLM Error: {}", e),
            usage: TokenUsage::default(),
        },
    }
}

fn parse_token_usage(body: &Value) -> TokenUsage {
    let usage = &body["usage"];
    TokenUsage {
        prompt_tokens: usage["prompt_tokens"].as_i64().unwrap_or(0),
        completion_tokens: usage["completion_tokens"].as_i64().unwrap_or(0),
        total_tokens: usage["total_tokens"].as_i64().unwrap_or(0),
    }
}

fn normalize_reasoning_effort(effort: &str) -> &'static str {
    match effort.trim().to_ascii_lowercase().as_str() {
        "xhigh" => "xhigh",
        "high" => "high",
        "medium" => "medium",
        "low" => "low",
        "minimal" => "minimal",
        _ => "none",
    }
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
    use super::{fallback_title, should_keep_full_card, CompressionLevel};

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
    fn compression_level_normalizes_settings() {
        assert_eq!(
            CompressionLevel::from_setting(" ULTRA ").as_setting(),
            "ultra"
        );
        assert_eq!(
            CompressionLevel::from_setting("unknown").as_setting(),
            "balanced"
        );
    }

    #[test]
    fn compression_level_selects_reasoning_effort() {
        assert_eq!(CompressionLevel::Light.reasoning_effort(), "minimal");
        assert_eq!(CompressionLevel::Balanced.reasoning_effort(), "low");
        assert_eq!(CompressionLevel::Strong.reasoning_effort(), "medium");
        assert_eq!(CompressionLevel::Ultra.reasoning_effort(), "high");
    }
}
