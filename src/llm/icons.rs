use crate::llm::transport::{LlmResponse, ReasoningConfig, TokenUsage};
use serde_json::Value;

pub async fn pick_topic_icon(
    topic_name: &str,
    allowlist: &[String],
    model: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
) -> LlmResponse {
    tracing::info!(topic_name, model, "LLM pick topic icon");
    let fallback = crate::domain::topic_visual::DEFAULT_TOPIC_ICON.to_string();
    if api_key.is_empty() || allowlist.is_empty() {
        return LlmResponse {
            content: fallback,
            usage: TokenUsage::default(),
        };
    }

    let icons_list = allowlist.join("\n");
    let prompt = format!(
        "Pick one icon for a learning topic named \"{topic_name}\".\n\
         Return JSON only: {{\"icon\":\"<exact-id-from-list>\"}}\n\
         Choose the single best semantic match from this allowlist:\n{icons_list}"
    );
    let response = crate::llm::transport::create_chat_completion(
        model,
        &prompt,
        api_key,
        api_base,
        reasoning,
        Some(64),
    )
    .await;
    if let Some(icon) = parse_topic_icon_response(&response.content) {
        if allowlist.iter().any(|candidate| candidate == &icon) {
            return LlmResponse {
                content: icon,
                usage: response.usage,
            };
        }
    }

    LlmResponse {
        content: fallback,
        usage: response.usage,
    }
}

fn parse_topic_icon_response(content: &str) -> Option<String> {
    let trimmed = content.trim();
    let json_text = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    let parsed = serde_json::from_str::<Value>(json_text).ok()?;
    parsed
        .get("icon")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::parse_topic_icon_response;

    #[test]
    fn parse_topic_icon_response_reads_json_object() {
        assert_eq!(
            parse_topic_icon_response("{\"icon\":\"lucide:code\"}"),
            Some("lucide:code".to_string())
        );
    }

    #[test]
    fn parse_topic_icon_response_reads_fenced_json() {
        assert_eq!(
            parse_topic_icon_response("```json\n{\"icon\":\"tabler:server\"}\n```"),
            Some("tabler:server".to_string())
        );
    }
}
