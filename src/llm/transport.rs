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
    /// True when the request failed (transport error, non-2xx status, or the
    /// model returned no usable content). `content` then holds a diagnostic
    /// message and must never be treated as model output.
    pub is_error: bool,
}

impl LlmResponse {
    pub(crate) fn error(message: String) -> Self {
        Self {
            content: message,
            usage: TokenUsage::default(),
            citations: Vec::new(),
            is_error: true,
        }
    }
}

/// Truncate a response body for log lines without splitting multi-byte chars.
pub(crate) fn content_snippet(content: &str, max_chars: usize) -> String {
    let trimmed = content.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars).collect();
    out.push('…');
    out
}

/// Largest provider error body embedded in a user-visible diagnostic. Bodies
/// beyond this are truncated so a misbehaving proxy cannot blow up the toast.
const MAX_ERROR_BODY_CHARS: usize = 1500;

/// Longest extracted provider error message kept in the headline.
const MAX_ERROR_MESSAGE_CHARS: usize = 500;

/// Format a non-2xx provider response into a usable diagnostic.
///
/// OpenAI-compatible providers return errors shaped like
/// `{"error":{"message":…,"type":…,"code":…}}`. Extract those fields into a
/// short headline instead of dumping the raw body; keep the raw body (bounded)
/// on a second line for debugging. Plain-text and unparseable bodies fall back
/// to the raw text inline.
fn format_http_error(status: reqwest::StatusCode, body: &str) -> String {
    let body = body.trim();
    let raw = content_snippet(body, MAX_ERROR_BODY_CHARS);
    let mut extracted = None::<(Option<String>, Option<String>, Option<String>)>;
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        if let Some(error) = value.get("error") {
            let field = |key: &str| {
                error
                    .get(key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_owned)
            };
            let message =
                field("message").map(|text| content_snippet(&text, MAX_ERROR_MESSAGE_CHARS));
            if message.is_some() || field("type").is_some() || field("code").is_some() {
                extracted = Some((message, field("type"), field("code")));
            }
        }
    }

    let headline = match extracted {
        Some((Some(message), Some(error_type), Some(code))) => {
            format!("LLM Error: HTTP {status} ({error_type}, {code}) {message}")
        }
        Some((Some(message), Some(error_type), None)) => {
            format!("LLM Error: HTTP {status} ({error_type}) {message}")
        }
        Some((Some(message), None, _)) => format!("LLM Error: HTTP {status} {message}"),
        Some((None, Some(error_type), Some(code))) => {
            format!("LLM Error: HTTP {status} ({error_type}, {code})")
        }
        Some((None, Some(error_type), None)) => format!("LLM Error: HTTP {status} ({error_type})"),
        Some((None, None, Some(code))) => format!("LLM Error: HTTP {status} (code={code})"),
        None | Some((None, None, None)) => {
            let mut head = format!("LLM Error: HTTP {status}");
            if !raw.is_empty() {
                head.push(' ');
                head.push_str(&raw);
            }
            head
        }
    };
    if body.is_empty() {
        return headline;
    }
    if headline.contains(&raw) {
        // Plain-text or already-inline body: the headline carries everything.
        return headline;
    }
    format!("{headline}\nRaw response: {raw}")
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
        model, prompt, api_key, api_base, reasoning, max_tokens, false, false,
    )
    .await
}

/// Same as [`create_chat_completion`] but asks the provider for strict JSON
/// output (`response_format: {"type": "json_object"}`). Only use for prompts
/// whose entire output must be a JSON document; providers that reject the field
/// are retried without it.
pub async fn create_chat_completion_json(
    model: &str,
    prompt: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
    max_tokens: Option<u32>,
) -> LlmResponse {
    create_chat_completion_grounded(
        model, prompt, api_key, api_base, reasoning, max_tokens, false, true,
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
                return LlmResponse::error(format_http_error(status, &error_body));
            }

            match res.json::<Value>().await {
                Ok(value) => {
                    let citations = extract_citations(&value);
                    let raw_content = extract_message_content(&value);
                    match serde_json::from_value::<CreateChatCompletionResponse>(value) {
                        Ok(response) => map_response(response, citations, raw_content),
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
///
/// When `json_object` is true the request asks the provider for
/// `response_format: {"type": "json_object"}`. Providers that reject the field
/// get a retry without it, so the flag is safe to enable for prompts that
/// demand raw JSON output.
#[allow(clippy::too_many_arguments)]
pub async fn create_chat_completion_grounded(
    model: &str,
    prompt: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
    max_tokens: Option<u32>,
    web_search: bool,
    json_object: bool,
) -> LlmResponse {
    tracing::info!(
        model,
        prompt_len = prompt.len(),
        ?max_tokens,
        web_search,
        json_object,
        "LLM chat completion request"
    );
    let started = std::time::Instant::now();
    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base(api_base);
    let client = Client::with_config(config);
    let base_url = client.config().api_base();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let mut structured = json_object;
    for attempt in 1..=MAX_LLM_ATTEMPTS {
        let body = build_chat_body(model, prompt, reasoning, max_tokens, web_search, structured);
        let duration_ms = started.elapsed().as_millis() as u64;
        match send_chat_request(&url, api_key, &body).await {
            Ok((status, value)) if status.is_success() => {
                let citations = extract_citations(&value);
                let raw_content = extract_message_content(&value);
                match serde_json::from_value::<CreateChatCompletionResponse>(value) {
                    Ok(response) => {
                        let llm_response = map_response(response, citations, raw_content);
                        tracing::info!(
                            model,
                            attempt,
                            duration_ms,
                            content_len = llm_response.content.len(),
                            is_error = llm_response.is_error,
                            "LLM chat completion response"
                        );
                        return llm_response;
                    }
                    Err(e) => {
                        let message = format!("LLM Error: {}", e);
                        tracing::warn!(
                            model,
                            attempt,
                            duration_ms,
                            error = %message,
                            "LLM chat completion response failed to deserialize"
                        );
                        return LlmResponse::error(message);
                    }
                }
            }
            Ok((status, value)) => {
                let status_code = status.as_u16();
                let error_body = value
                    .get("__error_body")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let unsupported_json_format = status_code == 400
                    && (error_body.contains("response_format")
                        || error_body.contains("json_object")
                        || error_body.to_ascii_lowercase().contains("response format"));
                if attempt < MAX_LLM_ATTEMPTS
                    && (status_code == 429 || status_code >= 500 || unsupported_json_format)
                {
                    if unsupported_json_format {
                        structured = false;
                        tracing::warn!(
                            model,
                            status = status_code,
                            duration_ms,
                            "provider rejected response_format json_object; retrying without it"
                        );
                    } else {
                        tracing::warn!(
                            model,
                            attempt,
                            status = status_code,
                            duration_ms,
                            retry_in_ms = RETRY_BACKOFF_MS[attempt - 1],
                            error = %content_snippet(error_body, 300),
                            "LLM chat completion failed; retrying"
                        );
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(
                        RETRY_BACKOFF_MS[attempt - 1],
                    ))
                    .await;
                    continue;
                }
                let message = format_http_error(status, error_body);
                tracing::warn!(
                    model,
                    attempt,
                    status = status_code,
                    duration_ms,
                    error = %content_snippet(error_body, 300),
                    "LLM chat completion failed"
                );
                return LlmResponse::error(message);
            }
            Err(err) => {
                let duration_ms = started.elapsed().as_millis() as u64;
                if attempt < MAX_LLM_ATTEMPTS {
                    tracing::warn!(
                        model,
                        attempt,
                        duration_ms,
                        retry_in_ms = RETRY_BACKOFF_MS[attempt - 1],
                        error = %err,
                        "LLM chat completion request failed; retrying"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(
                        RETRY_BACKOFF_MS[attempt - 1],
                    ))
                    .await;
                    continue;
                }
                tracing::warn!(
                    model,
                    attempt,
                    duration_ms,
                    error = %err,
                    "LLM chat completion request failed"
                );
                return LlmResponse::error(err);
            }
        }
    }
    unreachable!("attempt loop always returns")
}

const MAX_LLM_ATTEMPTS: usize = 3;
const RETRY_BACKOFF_MS: [u64; 2] = [1000, 2000];

/// One raw chat-completion HTTP round trip. Success returns the parsed response
/// JSON; non-2xx returns the status plus a small `__error_body` marker value;
/// network failures return `Err`.
async fn send_chat_request(
    url: &str,
    api_key: &str,
    body: &Value,
) -> Result<(reqwest::StatusCode, Value), String> {
    let http = http_client::shared();
    let response = http
        .post(url)
        .bearer_auth(api_key)
        .json(body)
        .send()
        .await
        .map_err(|err| format!("LLM Error: request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let error_body = response.text().await.unwrap_or_default();
        return Ok((status, json!({ "__error_body": error_body })));
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|err| format!("LLM Error: invalid response JSON: {err}"))?;
    Ok((status, value))
}

fn build_chat_body(
    model: &str,
    prompt: &str,
    reasoning: &ReasoningConfig,
    max_tokens: Option<u32>,
    web_search: bool,
    json_object: bool,
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

    if json_object {
        body["response_format"] = json!({ "type": "json_object" });
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

fn extract_message_content(value: &Value) -> Option<String> {
    let content = value
        .get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?;
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    content.as_array().map(|parts| {
        parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| part.as_str())
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn map_response(
    response: CreateChatCompletionResponse,
    citations: Vec<String>,
    raw_content: Option<String>,
) -> LlmResponse {
    let Some(content) = response
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .or(raw_content)
    else {
        tracing::warn!("LLM response carried no message content");
        return LlmResponse::error("LLM Error: model returned empty content".to_string());
    };
    if content.trim().is_empty() {
        tracing::warn!("LLM response content was empty");
        return LlmResponse::error("LLM Error: model returned empty content".to_string());
    }

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
        is_error: false,
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
    use serde_json::json;

    use super::{
        ReasoningConfig, build_chat_body, extract_message_content, format_http_error, map_response,
    };

    #[test]
    fn raw_content_supports_text_part_arrays() {
        let value = json!({
            "choices": [{"message": {"content": [{"type": "text", "text": "hello"}]}}]
        });

        assert_eq!(extract_message_content(&value).as_deref(), Some("hello"));
    }

    #[test]
    fn build_chat_body_uses_nested_reasoning_effort_xhigh() {
        let body = build_chat_body(
            "google/gemini-2.5-pro",
            "hello",
            &ReasoningConfig::new("xhigh"),
            None,
            false,
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
            false,
        );

        assert_eq!(body["max_tokens"], 1024);
    }

    #[test]
    fn build_chat_body_adds_json_object_response_format_when_requested() {
        let body = build_chat_body(
            "minimax/minimax-m3",
            "output json",
            &ReasoningConfig::new("none"),
            None,
            false,
            true,
        );

        assert_eq!(body["response_format"]["type"], "json_object");
        let plain = build_chat_body(
            "minimax/minimax-m3",
            "output json",
            &ReasoningConfig::new("none"),
            None,
            false,
            false,
        );
        assert!(plain.get("response_format").is_none());
    }

    #[test]
    fn format_http_error_extracts_provider_error_fields() {
        let message = format_http_error(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"Insufficient credits","type":"insufficient_quota","code":"insufficient_quota","param":null}}"#,
        );
        let (head, rest) = message.split_once('\n').unwrap_or((message.as_str(), ""));
        assert_eq!(
            head,
            "LLM Error: HTTP 429 Too Many Requests (insufficient_quota, insufficient_quota) Insufficient credits"
        );
        assert!(rest.starts_with("Raw response: {"));
        assert!(rest.contains("insufficient_quota"));
    }

    #[test]
    fn format_http_error_falls_back_to_plain_text_body() {
        let message = format_http_error(reqwest::StatusCode::BAD_REQUEST, "Too Many Requests");
        assert_eq!(message, "LLM Error: HTTP 400 Bad Request Too Many Requests");
    }

    #[test]
    fn format_http_error_handles_empty_body() {
        let message = format_http_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "");
        assert_eq!(message, "LLM Error: HTTP 500 Internal Server Error");
        let message = format_http_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "   ");
        assert_eq!(message, "LLM Error: HTTP 500 Internal Server Error");
    }

    #[test]
    fn format_http_error_truncates_oversized_bodies() {
        let body = format!(r#"{{"error":{{"message":"{}"}}}}"#, "x".repeat(10_000));
        let message = format_http_error(reqwest::StatusCode::BAD_REQUEST, &body);
        assert!(message.contains("LLM Error: HTTP 400"));
        assert!(message.chars().count() < 2_300);
        assert!(message.contains('…'));
    }

    #[test]
    fn format_http_error_inlines_unparseable_json_body() {
        let body = r#"{"detail":"upstream unavailable"}"#;
        let message = format_http_error(reqwest::StatusCode::BAD_GATEWAY, body);
        // No usable `error` object: the raw body stays on the headline and is
        // not duplicated on a second line.
        assert_eq!(message, format!("LLM Error: HTTP 502 Bad Gateway {body}"));
    }

    #[test]
    #[allow(deprecated)]
    fn map_response_flags_missing_content_as_error() {
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
            model: "minimax/minimax-m3".to_string(),
            system_fingerprint: None,
            object: "chat.completion".to_string(),
            usage: None,
            service_tier: None,
        };
        let result = map_response(response, Vec::new(), None);
        assert!(result.is_error);
        assert!(!result.content.contains("Failed parsing text"));
    }

    #[test]
    #[allow(deprecated)]
    fn map_response_flags_whitespace_content_as_error() {
        let response = async_openai::types::chat::CreateChatCompletionResponse {
            id: "test-id".to_string(),
            choices: vec![async_openai::types::chat::ChatChoice {
                index: 0,
                message: async_openai::types::chat::ChatCompletionResponseMessage {
                    content: Some("   \n  ".to_string()),
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
            model: "minimax/minimax-m3".to_string(),
            system_fingerprint: None,
            object: "chat.completion".to_string(),
            usage: None,
            service_tier: None,
        };
        let result = map_response(response, Vec::new(), None);
        assert!(result.is_error);
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

        let result = map_response(response, Vec::new(), None);

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

        let result = map_response(response, Vec::new(), None);

        assert!(result.is_error);
        assert_eq!(result.content, "LLM Error: model returned empty content");
        assert_eq!(result.usage.prompt_tokens, 0);
        assert_eq!(result.usage.completion_tokens, 0);
        assert_eq!(result.usage.total_tokens, 0);
    }
}
