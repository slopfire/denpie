use crate::llm::transport::{LlmResponse, ReasoningConfig, TokenUsage};
use serde_json::Value;
use std::collections::HashSet;

pub const TOPIC_ICON_SUGGESTION_COUNT: usize = 5;

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
            citations: Vec::new(),
            is_error: false,
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
                citations: Vec::new(),
                is_error: false,
            };
        }
    }

    LlmResponse {
        content: fallback,
        usage: response.usage,
        citations: Vec::new(),
        is_error: false,
    }
}

pub async fn suggest_topic_icons(
    topic_name: &str,
    allowlist: &[String],
    model: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
) -> LlmResponse {
    tracing::info!(topic_name, model, "LLM suggest topic icons");
    let fallback = finalize_suggestions(Vec::new(), allowlist);
    if api_key.is_empty() || allowlist.is_empty() {
        return LlmResponse {
            content: serde_json::to_string(&fallback).unwrap_or_default(),
            usage: TokenUsage::default(),
            citations: Vec::new(),
            is_error: false,
        };
    }

    let icons_list = allowlist.join("\n");
    let prompt = format!(
        "Suggest {count} icons for a learning topic named \"{topic_name}\".\n\
         Return JSON only: {{\"icons\":[\"<exact-id-from-list>\", ...]}}\n\
         Choose {count} distinct best semantic matches from this allowlist:\n{icons_list}",
        count = TOPIC_ICON_SUGGESTION_COUNT,
    );
    let response = crate::llm::transport::create_chat_completion(
        model,
        &prompt,
        api_key,
        api_base,
        reasoning,
        Some(256),
    )
    .await;
    let icons = finalize_suggestions(parse_topic_icons_response(&response.content), allowlist);
    LlmResponse {
        content: serde_json::to_string(&icons).unwrap_or_default(),
        usage: response.usage,
        citations: Vec::new(),
        is_error: false,
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

fn parse_topic_icons_response(content: &str) -> Vec<String> {
    let trimmed = content.trim();
    let json_text = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    let Ok(parsed) = serde_json::from_str::<Value>(json_text) else {
        return Vec::new();
    };
    parsed
        .get("icons")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Keep only allowlisted suggestions, deduplicated in order, then top up with
/// deterministic allowlist icons so exactly `TOPIC_ICON_SUGGESTION_COUNT` are
/// offered even when the model returns few or invalid ids.
fn finalize_suggestions(parsed: Vec<String>, allowlist: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut icons: Vec<String> = parsed
        .into_iter()
        .filter(|icon| allowlist.iter().any(|candidate| candidate == icon))
        .filter(|icon| seen.insert(icon.clone()))
        .take(TOPIC_ICON_SUGGESTION_COUNT)
        .collect();
    for candidate in allowlist {
        if icons.len() >= TOPIC_ICON_SUGGESTION_COUNT {
            break;
        }
        if seen.insert(candidate.clone()) {
            icons.push(candidate.clone());
        }
    }
    icons
}

#[cfg(test)]
mod tests {
    use super::{
        TOPIC_ICON_SUGGESTION_COUNT, finalize_suggestions, parse_topic_icon_response,
        parse_topic_icons_response,
    };

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

    #[test]
    fn parse_topic_icons_response_reads_array() {
        assert_eq!(
            parse_topic_icons_response("{\"icons\":[\"lucide:code\",\"tabler:server\"]}"),
            vec!["lucide:code".to_string(), "tabler:server".to_string()]
        );
    }

    #[test]
    fn parse_topic_icons_response_reads_fenced_json() {
        assert_eq!(
            parse_topic_icons_response("```json\n{\"icons\":[\"lucide:code\"]}\n```"),
            vec!["lucide:code".to_string()]
        );
    }

    #[test]
    fn parse_topic_icons_response_ignores_non_json() {
        assert!(parse_topic_icons_response("sure, here you go").is_empty());
    }

    #[test]
    fn finalize_suggestions_filters_and_dedups() {
        let allowlist = ["a", "b", "c", "d", "e", "f"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let parsed = vec![
            "b".to_string(),
            "zzz".to_string(),
            "b".to_string(),
            "a".to_string(),
        ];
        assert_eq!(
            finalize_suggestions(parsed, &allowlist),
            vec!["b", "a", "c", "d", "e"]
        );
    }

    #[test]
    fn finalize_suggestions_tops_up_and_caps_at_five() {
        let allowlist = ["a", "b", "c", "d", "e", "f"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(
            finalize_suggestions(Vec::new(), &allowlist),
            vec!["a", "b", "c", "d", "e"]
        );
        let many = (0..10).map(|i| format!("a{i}")).collect::<Vec<_>>();
        assert_eq!(
            finalize_suggestions(many, &allowlist).len(),
            TOPIC_ICON_SUGGESTION_COUNT
        );
    }
}
