use serde_json::{json, Value};

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
    reasoning: &ReasoningConfig,
) -> LlmResponse {
    if api_key.is_empty() {
        return LlmResponse {
            content: format!("Compressed: {}", full_content),
            usage: TokenUsage::default(),
        };
    }

    let prompt = format!(
        "Compress this tip into a very short summary:\n\n{}",
        full_content
    );
    create_chat_completion(model, &prompt, api_key, api_base, reasoning).await
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
    use super::fallback_title;

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
}
