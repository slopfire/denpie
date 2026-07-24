use crate::http_client;
use async_openai::{
    Client,
    config::{Config, OpenAIConfig},
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionResponse,
    },
};
use serde_json::{Value, json};

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
    pub citations: Vec<String>,
}

impl LlmResponse {
    fn error(message: String) -> Self {
        Self {
            content: message,
            usage: TokenUsage::default(),
            citations: Vec::new(),
        }
    }
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

pub async fn create_chat_completion(
    model: &str,
    prompt: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
    max_tokens: Option<u32>,
) -> LlmResponse {
    create_chat_completion_grounded(
        model, prompt, api_key, api_base, reasoning, max_tokens, false,
    )
    .await
}

/// Send a vision request: an image (as a data URL) plus a text prompt, using the
/// OpenAI-compatible chat completions format with `image_url` content. Returns
/// the model's text response. Uses a direct JSON payload (not async-openai's
/// typed builder) because the typed builder's image support is cumbersome.
pub async fn create_vision_completion(
    model: &str,
    prompt: &str,
    image_data_url: &str,
    api_key: &str,
    api_base: &str,
    max_tokens: Option<u32>,
) -> LlmResponse {
    tracing::info!(
        model,
        prompt_len = prompt.len(),
        image_len = image_data_url.len(),
        ?max_tokens,
        "LLM vision completion request"
    );
    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base(api_base);
    let client = Client::with_config(config);
    let base_url = client.config().api_base();

    let body = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {"type": "image_url", "image_url": {"url": image_data_url}}
            ]
        }]
    });

    let mut body = body;
    if let Some(limit) = max_tokens {
        body["max_tokens"] = json!(limit);
    }

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let http = http_client::shared();

    match http.post(url).bearer_auth(api_key).json(&body).send().await {
        Ok(res) => {
            if !res.status().is_success() {
                let status = res.status();
                let error_body = res.text().await.unwrap_or_default();
                return LlmResponse::error(format!("LLM Error: HTTP {} {}", status, error_body));
            }

            match res.json::<Value>().await {
                Ok(value) => {
                    let citations = extract_citations(&value);
                    match serde_json::from_value::<CreateChatCompletionResponse>(value) {
                        Ok(response) => map_response(response, citations),
                        Err(e) => LlmResponse::error(format!("LLM Error: {}", e)),
                    }
                }
                Err(e) => LlmResponse::error(format!("LLM Error: {}", e)),
            }
        }
        Err(e) => LlmResponse::error(format!("LLM Error: {}", e)),
    }
}

/// Same as [`create_chat_completion`] but optionally injects OpenRouter's
/// server-side web grounding plugin. When `web_search` is true and the provider
/// supports it, the response carries inline URL citations in `LlmResponse::citations`.
/// For providers that ignore the `plugins` field the flag is harmless.
pub async fn create_chat_completion_grounded(
    model: &str,
    prompt: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
    max_tokens: Option<u32>,
    web_search: bool,
) -> LlmResponse {
    tracing::info!(
        model,
        prompt_len = prompt.len(),
        ?max_tokens,
        web_search,
        "LLM chat completion request"
    );
    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base(api_base);
    let client = Client::with_config(config);
    let base_url = client.config().api_base();

    let body = build_chat_body(model, prompt, reasoning, max_tokens, web_search);
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let http = http_client::shared();

    match http.post(url).bearer_auth(api_key).json(&body).send().await {
        Ok(res) => {
            if !res.status().is_success() {
                let status = res.status();
                let error_body = res.text().await.unwrap_or_default();
                return LlmResponse::error(format!("LLM Error: HTTP {} {}", status, error_body));
            }

            match res.json::<Value>().await {
                Ok(value) => {
                    let citations = extract_citations(&value);
                    match serde_json::from_value::<CreateChatCompletionResponse>(value) {
                        Ok(response) => map_response(response, citations),
                        Err(e) => LlmResponse::error(format!("LLM Error: {}", e)),
                    }
                }
                Err(e) => LlmResponse::error(format!("LLM Error: {}", e)),
            }
        }
        Err(e) => LlmResponse::error(format!("LLM Error: {}", e)),
    }
}

fn build_chat_body(
    model: &str,
    prompt: &str,
    reasoning: &ReasoningConfig,
    max_tokens: Option<u32>,
    web_search: bool,
) -> Value {
    let effort = normalize_reasoning_effort(&reasoning.effort);
    let message = ChatCompletionRequestUserMessageArgs::default()
        .content(prompt)
        .build()
        .map_or_else(
            |_| json!({"role": "user", "content": prompt}),
            |user_msg| {
                let wrapped = ChatCompletionRequestMessage::User(user_msg);
                serde_json::to_value(&wrapped)
                    .unwrap_or_else(|_| json!({"role": "user", "content": prompt}))
            },
        );

    let mut body = json!({
        "model": model,
        "messages": [message],
        "reasoning": {
            "effort": effort
        }
    });

    if let Some(limit) = max_tokens {
        body["max_tokens"] = json!(limit);
    }

    if web_search {
        body["plugins"] = json!([{ "id": "web", "max_results": 5 }]);
    }

    body
}

/// Extract URL citations from the raw chat-completion response. OpenRouter returns
/// them under `choices[0].message.annotations[].url_citation.url`; these may not
/// deserialize cleanly into the typed struct, so read them from the `Value` first.
fn extract_citations(value: &Value) -> Vec<String> {
    value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("annotations"))
        .and_then(Value::as_array)
        .map(|annotations| {
            annotations
                .iter()
                .filter_map(|annotation| {
                    annotation
                        .get("url_citation")
                        .and_then(|citation| citation.get("url"))
                        .or_else(|| annotation.get("url"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn map_response(response: CreateChatCompletionResponse, citations: Vec<String>) -> LlmResponse {
    let content = response
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .unwrap_or_else(|| "Failed parsing text".to_string());

    let usage = response
        .usage
        .map(|usage| TokenUsage {
            prompt_tokens: i64::from(usage.prompt_tokens),
            completion_tokens: i64::from(usage.completion_tokens),
            total_tokens: i64::from(usage.total_tokens),
        })
        .unwrap_or_default();

    LlmResponse {
        content,
        usage,
        citations,
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

#[cfg(test)]
mod tests {
    use super::{ReasoningConfig, build_chat_body, map_response};

    #[test]
    fn build_chat_body_uses_nested_reasoning_effort_xhigh() {
        let body = build_chat_body(
            "google/gemini-2.5-pro",
            "hello",
            &ReasoningConfig::new("xhigh"),
            None,
            false,
        );

        assert_eq!(body["model"], "google/gemini-2.5-pro");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
        assert_eq!(body["reasoning"]["effort"], "xhigh");
        assert!(body.get("reasoning_effort").is_none());
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn build_chat_body_uses_nested_reasoning_effort_none() {
        let body = build_chat_body(
            "google/gemini-2.5-pro",
            "hello",
            &ReasoningConfig::new("none"),
            None,
            false,
        );

        assert_eq!(body["reasoning"]["effort"], "none");
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn build_chat_body_includes_max_tokens_when_provided() {
        let body = build_chat_body(
            "google/gemini-2.5-pro",
            "hello",
            &ReasoningConfig::new("none"),
            Some(1024),
            false,
        );

        assert_eq!(body["max_tokens"], 1024);
    }

    #[test]
    #[allow(deprecated)]
    fn map_response_extracts_content_and_usage() {
        let response = async_openai::types::chat::CreateChatCompletionResponse {
            id: "test-id".to_string(),
            choices: vec![async_openai::types::chat::ChatChoice {
                index: 0,
                message: async_openai::types::chat::ChatCompletionResponseMessage {
                    content: Some("tip content".to_string()),
                    refusal: None,
                    tool_calls: None,
                    annotations: None,
                    role: async_openai::types::chat::Role::Assistant,
                    function_call: None,
                    audio: None,
                },
                finish_reason: None,
                logprobs: None,
            }],
            created: 0,
            model: "m".to_string(),
            service_tier: None,
            system_fingerprint: None,
            object: "chat.completion".to_string(),
            usage: Some(async_openai::types::chat::CompletionUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                prompt_tokens_details: None,
                completion_tokens_details: None,
            }),
        };

        let result = map_response(response, Vec::new());

        assert_eq!(result.content, "tip content");
        assert_eq!(result.usage.prompt_tokens, 10);
        assert_eq!(result.usage.completion_tokens, 5);
        assert_eq!(result.usage.total_tokens, 15);
    }

    #[test]
    #[allow(deprecated)]
    fn map_response_uses_fallback_when_content_missing_and_usage_absent() {
        let response = async_openai::types::chat::CreateChatCompletionResponse {
            id: "test-id".to_string(),
            choices: vec![async_openai::types::chat::ChatChoice {
                index: 0,
                message: async_openai::types::chat::ChatCompletionResponseMessage {
                    content: None,
                    refusal: None,
                    tool_calls: None,
                    annotations: None,
                    role: async_openai::types::chat::Role::Assistant,
                    function_call: None,
                    audio: None,
                },
                finish_reason: None,
                logprobs: None,
            }],
            created: 0,
            model: "m".to_string(),
            service_tier: None,
            system_fingerprint: None,
            object: "chat.completion".to_string(),
            usage: None,
        };

        let result = map_response(response, Vec::new());

        assert_eq!(result.content, "Failed parsing text");
        assert_eq!(result.usage.prompt_tokens, 0);
        assert_eq!(result.usage.completion_tokens, 0);
        assert_eq!(result.usage.total_tokens, 0);
    }
}
