use async_openai::{config::OpenAIConfig, Client};
use serde_json::json;

pub async fn generate_new_card(topic: &str, model: &str, template: &str, api_key: &str) -> String {
    if api_key.is_empty() { return format!("Generated tip for {} (API KEY MISSING)", topic); }

    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base("https://openrouter.ai/api/v1");
    let client = Client::with_config(config);

    let prompt = template.replace("{topic}", topic);

    let req = json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}]
    });

    match client.chat().create_byot::<serde_json::Value>(req).await {
        Ok(res) => res["choices"][0]["message"]["content"].as_str().unwrap_or("Failed parsing text").to_string(),
        Err(e) => format!("LLM Error: {}", e)
    }
}

pub async fn compress_card(full_content: &str, api_key: &str) -> String {
    if api_key.is_empty() { return format!("Compressed: {}", full_content); }

    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base("https://openrouter.ai/api/v1");
    let client = Client::with_config(config);

    let req = json!({
        "model": "google/gemini-3.1-flash-lite-preview",
        "messages": [{"role": "user", "content": format!("Compress this tip into a very short summary:\n\n{}", full_content)}]
    });

    match client.chat().create_byot::<serde_json::Value>(req).await {
        Ok(res) => res["choices"][0]["message"]["content"].as_str().unwrap_or("Failed parsing text").to_string(),
        Err(e) => format!("LLM Error: {}", e)
    }
}
