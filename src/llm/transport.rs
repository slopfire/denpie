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
    /// True when the provider reported `finish_reason=length`: the completion
    /// token budget ran out mid-generation, so trailing output (typically a
    /// JSON batch) is cut off.
    pub truncated: bool,
}

impl LlmResponse {
    pub(crate) fn error(message: String) -> Self {
        Self {
            content: message,
            usage: TokenUsage::default(),
            citations: Vec::new(),
            is_error: true,
            truncated: false,
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

/// Join an error and its `source` chain. reqwest 0.13's `Display` is only the
/// kind (`error decoding response body`) and drops the timeout / parse cause.
fn error_chain(err: &dyn std::error::Error) -> String {
    let mut parts = Vec::new();
    let mut current = Some(err);
    while let Some(inner) = current {
        let text = inner.to_string();
        if parts.last().is_none_or(|last| last != &text) {
            parts.push(text);
        }
        current = inner.source();
    }
    parts.join(": ")
}

/// Bounded body for logs: keep the head and tail so a truncated JSON object
/// still shows how it started and where it broke.
fn response_body_snippet(body: &str) -> String {
    const MAX: usize = 1500;
    const HEAD: usize = 700;
    const TAIL: usize = 700;
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "(empty)".to_string();
    }
    let count = trimmed.chars().count();
    if count <= MAX {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(HEAD).collect();
    let tail: String = trimmed
        .chars()
        .rev()
        .take(TAIL)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{head}…<truncated {count} chars>…{tail}")
}

fn format_reqwest_error(context: &str, err: &reqwest::Error) -> ChatTransportError {
    let chain = error_chain(err);
    if err.is_timeout() {
        ChatTransportError {
            message: format!("LLM Error: {context} timed out: {chain}"),
            retryable: false,
        }
    } else {
        ChatTransportError {
            message: format!("LLM Error: {context}: {chain}"),
            retryable: true,
        }
    }
}

fn parse_llm_json_body(
    status: reqwest::StatusCode,
    content_type: Option<&str>,
    body: &str,
) -> Result<Value, String> {
    match serde_json::from_str::<Value>(body) {
        Ok(value) => Ok(value),
        Err(err) => {
            let snippet = response_body_snippet(body);
            let content_type = content_type.filter(|value| !value.is_empty());
            tracing::warn!(
                status = status.as_u16(),
                content_type,
                body_len = body.len(),
                error = %err,
                body = %snippet,
                "LLM response body was not valid JSON"
            );
            Err(format!(
                "LLM Error: invalid response JSON: {err} (status={}, content_type={}, body_len={}, body={snippet})",
                status.as_u16(),
                content_type.unwrap_or("missing"),
                body.len(),
            ))
        }
    }
}

fn describe_empty_content(value: &Value) -> String {
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first());
    let message = choice.and_then(|choice| choice.get("message"));
    let finish_reason = choice
        .and_then(|choice| choice.get("finish_reason"))
        .and_then(Value::as_str)
        .unwrap_or("missing");
    let content_kind = match message.and_then(|message| message.get("content")) {
        None => "missing",
        Some(Value::Null) => "null",
        Some(Value::String(text)) if text.trim().is_empty() => "empty_string",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(_) => "other",
    };
    let message_keys = message
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect::<Vec<_>>().join(","))
        .unwrap_or_default();
    format!("finish_reason={finish_reason} content={content_kind} message_keys={message_keys}")
}

#[derive(Debug)]
struct ChatTransportError {
    message: String,
    retryable: bool,
}

impl std::fmt::Display for ChatTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
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
///
/// Always sends OpenRouter's `reasoning.effort` so thinking models (MiniMax-M3,
/// Gemini thinking, etc.) do not spend `max_tokens` on hidden reasoning and
/// return `content=null` with `finish_reason=length`.
pub async fn create_vision_completion(
    model: &str,
    prompt: &str,
    image_data_url: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
    max_tokens: Option<u32>,
) -> LlmResponse {
    tracing::info!(
        model,
        prompt_len = prompt.len(),
        image_len = image_data_url.len(),
        reasoning_effort = %reasoning.effort,
        ?max_tokens,
        "LLM vision completion request"
    );
    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base(api_base);
    let client = Client::with_config(config);
    let base_url = client.config().api_base();

    let body = build_vision_body(model, prompt, image_data_url, reasoning, max_tokens);

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    match send_chat_request(&url, api_key, &body).await {
        Ok((status, value)) if status.is_success() => {
            let citations = extract_citations(&value);
            let raw_content = extract_message_content(&value);
            let empty_detail = describe_empty_content(&value);
            match serde_json::from_value::<CreateChatCompletionResponse>(value) {
                Ok(response) => {
                    map_response_with_detail(response, citations, raw_content, &empty_detail)
                }
                Err(e) => LlmResponse::error(format!("LLM Error: {e}")),
            }
        }
        Ok((status, value)) => {
            let error_body = value
                .get("__error_body")
                .and_then(Value::as_str)
                .unwrap_or("");
            LlmResponse::error(format_http_error(status, error_body))
        }
        Err(err) => LlmResponse::error(err.message),
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
                let empty_detail = describe_empty_content(&value);
                match serde_json::from_value::<CreateChatCompletionResponse>(value) {
                    Ok(response) => {
                        let llm_response = map_response_with_detail(
                            response,
                            citations,
                            raw_content,
                            &empty_detail,
                        );
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
                if err.retryable && attempt < MAX_LLM_ATTEMPTS {
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
                return LlmResponse::error(err.message);
            }
        }
    }
    unreachable!("attempt loop always returns")
}

const MAX_LLM_ATTEMPTS: usize = 3;
const RETRY_BACKOFF_MS: [u64; 2] = [1000, 2000];

/// One raw chat-completion HTTP round trip. Success returns the parsed response
/// JSON; non-2xx returns the status plus a small `__error_body` marker value;
/// network / parse failures return `Err` with a retry policy.
async fn send_chat_request(
    url: &str,
    api_key: &str,
    body: &Value,
) -> Result<(reqwest::StatusCode, Value), ChatTransportError> {
    let http = http_client::llm();
    let response = http
        .post(url)
        .bearer_auth(api_key)
        .json(body)
        .send()
        .await
        .map_err(|err| format_reqwest_error("request failed", &err))?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let content_length = response.content_length();
    let body = response.text().await.map_err(|err| {
        tracing::warn!(
            status = status.as_u16(),
            content_type = content_type.as_deref(),
            ?content_length,
            error = %error_chain(&err),
            timed_out = err.is_timeout(),
            "LLM response body read failed"
        );
        format_reqwest_error("failed to read response body", &err)
    })?;
    if !status.is_success() {
        return Ok((status, json!({ "__error_body": body })));
    }
    tracing::debug!(
        status = status.as_u16(),
        content_type = content_type.as_deref(),
        body_len = body.len(),
        body = %response_body_snippet(&body),
        "LLM chat completion raw response"
    );
    parse_llm_json_body(status, content_type.as_deref(), &body)
        .map_err(|message| ChatTransportError {
            message,
            retryable: true,
        })
        .map(|value| (status, value))
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

    // OpenRouter's documented way to disable thinking models' hidden reasoning
    // is `enabled: false`; an `effort` value alone still lets mandatory
    // reasoning models burn the completion budget before any visible content.
    let reasoning_block = if effort == "none" {
        json!({ "enabled": false })
    } else {
        json!({ "effort": effort })
    };

    let mut body = json!({
        "model": model,
        "messages": [message],
        "reasoning": reasoning_block
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

fn build_vision_body(
    model: &str,
    prompt: &str,
    image_data_url: &str,
    reasoning: &ReasoningConfig,
    max_tokens: Option<u32>,
) -> Value {
    let effort = normalize_reasoning_effort(&reasoning.effort);
    // Same rationale as `build_chat_body`: `enabled: false` is the reliable
    // way to stop thinking models from spending max_tokens on hidden
    // reasoning instead of the visible answer.
    let reasoning_block = if effort == "none" {
        json!({ "enabled": false })
    } else {
        json!({ "effort": effort })
    };
    let mut body = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {"type": "image_url", "image_url": {"url": image_data_url}}
            ]
        }],
        "reasoning": reasoning_block
    });
    if let Some(limit) = max_tokens {
        body["max_tokens"] = json!(limit);
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

fn map_response_with_detail(
    response: CreateChatCompletionResponse,
    citations: Vec<String>,
    raw_content: Option<String>,
    empty_detail: &str,
) -> LlmResponse {
    let truncated = response
        .choices
        .first()
        .and_then(|choice| choice.finish_reason.as_ref())
        == Some(&async_openai::types::chat::FinishReason::Length);

    let Some(content) = response
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .or(raw_content)
    else {
        if empty_detail.is_empty() {
            tracing::warn!("LLM response carried no message content");
        } else {
            tracing::warn!(detail = %empty_detail, "LLM response carried no message content");
        }
        return LlmResponse::error(empty_content_error(empty_detail));
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
        truncated,
    }
}

fn empty_content_error(empty_detail: &str) -> String {
    if empty_detail.contains("finish_reason=length") {
        "LLM Error: model hit the completion token limit before producing content".to_string()
    } else if empty_detail.is_empty() {
        "LLM Error: model returned empty content".to_string()
    } else {
        format!("LLM Error: model returned empty content ({empty_detail})")
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
        ReasoningConfig, build_chat_body, build_vision_body, describe_empty_content,
        empty_content_error, extract_message_content, format_http_error, map_response_with_detail,
        parse_llm_json_body, response_body_snippet,
    };

    #[test]
    fn raw_content_supports_text_part_arrays() {
        let value = json!({
            "choices": [{"message": {"content": [{"type": "text", "text": "hello"}]}}]
        });

        assert_eq!(extract_message_content(&value).as_deref(), Some("hello"));
    }

    #[test]
    fn response_body_snippet_keeps_head_and_tail() {
        let body = format!("{}MIDDLE{}", "A".repeat(800), "Z".repeat(800));
        let snippet = response_body_snippet(&body);
        assert!(snippet.starts_with('A'));
        assert!(snippet.ends_with('Z'));
        assert!(snippet.contains("truncated"));
        assert!(snippet.contains("1606 chars"));
    }

    #[test]
    fn build_vision_body_disables_reasoning_with_enabled_false() {
        let body = build_vision_body(
            "minimax/minimax-m3",
            "what color?",
            "data:image/png;base64,abc",
            &ReasoningConfig::new("none"),
            Some(1024),
        );

        assert_eq!(body["model"], "minimax/minimax-m3");
        assert_eq!(body["reasoning"]["enabled"], false);
        assert!(body["reasoning"].get("effort").is_none());
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(body["messages"][0]["content"][1]["type"], "image_url");
    }
    #[test]
    fn parse_llm_json_body_includes_status_and_snippet() {
        let err = parse_llm_json_body(
            reqwest::StatusCode::OK,
            Some("text/html; charset=utf-8"),
            "<html>nope</html>",
        )
        .unwrap_err();
        assert!(err.contains("invalid response JSON"));
        assert!(err.contains("status=200"));
        assert!(err.contains("text/html; charset=utf-8"));
        assert!(err.contains("<html>nope</html>"));
    }

    #[test]
    fn empty_content_error_explains_length_truncation() {
        assert_eq!(
            empty_content_error("finish_reason=length content=null message_keys=content,reasoning"),
            "LLM Error: model hit the completion token limit before producing content"
        );
        assert_eq!(
            empty_content_error(""),
            "LLM Error: model returned empty content"
        );
        assert!(
            empty_content_error("finish_reason=stop content=null message_keys=content")
                .contains("finish_reason=stop")
        );
    }

    #[test]
    fn extract_message_content_ignores_reasoning_when_content_is_null() {
        let value = json!({
            "choices": [{
                "message": {
                    "content": null,
                    "reasoning": "the pixel is red"
                }
            }]
        });
        assert_eq!(extract_message_content(&value), None);
    }

    #[test]
    fn describe_empty_content_reports_null_content_and_keys() {
        let value = json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "reasoning": "thinking"
                }
            }]
        });
        let detail = describe_empty_content(&value);
        assert!(detail.contains("finish_reason=stop"));
        assert!(detail.contains("content=null"));
        assert!(detail.contains("reasoning"));
    }

    #[test]
    fn build_chat_body_disables_reasoning_with_enabled_false() {
        let body = build_chat_body(
            "google/gemini-2.5-pro",
            "hello",
            &ReasoningConfig::new("none"),
            None,
            false,
            false,
        );

        assert_eq!(body["reasoning"]["enabled"], false);
        assert!(body["reasoning"].get("effort").is_none());
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn build_chat_body_keeps_explicit_reasoning_effort() {
        let body = build_chat_body(
            "google/gemini-2.5-pro",
            "hello",
            &ReasoningConfig::new("low"),
            None,
            false,
            false,
        );

        assert_eq!(body["reasoning"]["effort"], "low");
        assert!(body["reasoning"].get("enabled").is_none());
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
        let result = map_response_with_detail(response, Vec::new(), None, "");
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
        let result = map_response_with_detail(response, Vec::new(), None, "");
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

        let result = map_response_with_detail(response, Vec::new(), None, "");

        assert_eq!(result.content, "tip content");
        assert_eq!(result.usage.prompt_tokens, 10);
        assert_eq!(result.usage.completion_tokens, 5);
        assert_eq!(result.usage.total_tokens, 15);
    }

    #[test]
    #[allow(deprecated)]
    fn map_response_marks_length_finish_reason_as_truncated() {
        let make = |finish_reason: Option<async_openai::types::chat::FinishReason>| {
            async_openai::types::chat::CreateChatCompletionResponse {
                id: "test-id".to_string(),
                choices: vec![async_openai::types::chat::ChatChoice {
                    index: 0,
                    message: async_openai::types::chat::ChatCompletionResponseMessage {
                        content: Some("{\"cards\": [{\"title\": \"cut".to_string()),
                        refusal: None,
                        tool_calls: None,
                        annotations: None,
                        role: async_openai::types::chat::Role::Assistant,
                        function_call: None,
                        audio: None,
                    },
                    finish_reason,
                    logprobs: None,
                }],
                created: 0,
                model: "m".to_string(),
                service_tier: None,
                system_fingerprint: None,
                object: "chat.completion".to_string(),
                usage: None,
            }
        };

        let truncated = map_response_with_detail(
            make(Some(async_openai::types::chat::FinishReason::Length)),
            Vec::new(),
            None,
            "",
        );
        assert!(!truncated.is_error);
        assert!(truncated.truncated);

        let complete = map_response_with_detail(
            make(Some(async_openai::types::chat::FinishReason::Stop)),
            Vec::new(),
            None,
            "",
        );
        assert!(!complete.is_error);
        assert!(!complete.truncated);
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

        let result = map_response_with_detail(response, Vec::new(), None, "");

        assert!(result.is_error);
        assert_eq!(result.content, "LLM Error: model returned empty content");
        assert_eq!(result.usage.prompt_tokens, 0);
        assert_eq!(result.usage.completion_tokens, 0);
        assert_eq!(result.usage.total_tokens, 0);
    }
}
