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
) -> LlmResponse {
    tracing::info!(
        model,
        prompt_len = prompt.len(),
        "LLM chat completion request"
    );
    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base(api_base);
    let client = Client::with_config(config);
    let base_url = client.config().api_base();

    let body = build_chat_body(model, prompt, reasoning);
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let http = http_client::shared();

    match http.post(url).bearer_auth(api_key).json(&body).send().await {
        Ok(res) => {
            if !res.status().is_success() {
                let status = res.status();
                let error_body = res.text().await.unwrap_or_default();
                return LlmResponse {
                    content: format!("LLM Error: HTTP {} {}", status, error_body),
                    usage: TokenUsage::default(),
                };
            }

            match res.json::<Value>().await {
                Ok(value) => match serde_json::from_value::<CreateChatCompletionResponse>(value) {
                    Ok(response) => map_response(response),
                    Err(e) => LlmResponse {
                        content: format!("LLM Error: {}", e),
                        usage: TokenUsage::default(),
                    },
                },
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

fn build_chat_body(model: &str, prompt: &str, reasoning: &ReasoningConfig) -> Value {
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

    json!({
        "model": model,
        "messages": [message],
        "reasoning": {
            "effort": effort
        }
    })
}

fn map_response(response: CreateChatCompletionResponse) -> LlmResponse {
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

    LlmResponse { content, usage }
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
        );

        assert_eq!(body["model"], "google/gemini-2.5-pro");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
        assert_eq!(body["reasoning"]["effort"], "xhigh");
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn build_chat_body_uses_nested_reasoning_effort_none() {
        let body = build_chat_body(
            "google/gemini-2.5-pro",
            "hello",
            &ReasoningConfig::new("none"),
        );

        assert_eq!(body["reasoning"]["effort"], "none");
        assert!(body.get("reasoning_effort").is_none());
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

        let result = map_response(response);

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

        let result = map_response(response);

        assert_eq!(result.content, "Failed parsing text");
        assert_eq!(result.usage.prompt_tokens, 0);
        assert_eq!(result.usage.completion_tokens, 0);
        assert_eq!(result.usage.total_tokens, 0);
    }
}
