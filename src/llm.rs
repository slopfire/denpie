use serde_json::{json, Value};

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
    topic: &str,
    model: &str,
    template: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
) -> String {
    if api_key.is_empty() {
        return format!("Generated tip for {} (API KEY MISSING)", topic);
    }

    let prompt = template.replace("{topic}", topic);
    create_chat_completion(model, &prompt, api_key, api_base, reasoning).await
}

pub async fn compress_card(
    full_content: &str,
    model: &str,
    api_key: &str,
    api_base: &str,
    reasoning: &ReasoningConfig,
) -> String {
    if api_key.is_empty() {
        return format!("Compressed: {}", full_content);
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
) -> String {
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
                return format!("LLM Error: HTTP {} {}", status, body);
            }

            match res.json::<Value>().await {
                Ok(body) => body["choices"]
                    .as_array()
                    .and_then(|choices| choices.first())
                    .and_then(|choice| choice["message"]["content"].as_str())
                    .map(str::to_string)
                    .unwrap_or("Failed parsing text".to_string()),
                Err(e) => format!("LLM Error: {}", e),
            }
        }
        Err(e) => format!("LLM Error: {}", e),
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
