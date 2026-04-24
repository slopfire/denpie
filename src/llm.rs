use async_openai::{config::OpenAIConfig, Client, types::{CreateChatCompletionRequestArgs, ChatCompletionRequestUserMessageArgs}};

pub async fn generate_new_card(topic: &str, model: &str, template: &str, api_key: &str, api_base: &str) -> String {
    if api_key.is_empty() { return format!("Generated tip for {} (API KEY MISSING)", topic); }

    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base(api_base);
    let client = Client::with_config(config);

    let prompt = template.replace("{topic}", topic);

    let req = CreateChatCompletionRequestArgs::default()
        .model(model)
        .messages([
            ChatCompletionRequestUserMessageArgs::default()
                .content(prompt)
                .build().unwrap()
                .into()
        ])
        .build().unwrap();

    match client.chat().create(req).await {
        Ok(res) => res.choices.into_iter().next().and_then(|c| c.message.content).unwrap_or("Failed parsing text".to_string()),
        Err(e) => format!("LLM Error: {}", e)
    }
}

pub async fn compress_card(full_content: &str, api_key: &str, api_base: &str) -> String {
    if api_key.is_empty() { return format!("Compressed: {}", full_content); }

    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base(api_base);
    let client = Client::with_config(config);

    let req = CreateChatCompletionRequestArgs::default()
        .model("google/gemini-3.1-flash-lite-preview")
        .messages([
            ChatCompletionRequestUserMessageArgs::default()
                .content(format!("Compress this tip into a very short summary:\n\n{}", full_content))
                .build().unwrap()
                .into()
        ])
        .build().unwrap();

    match client.chat().create(req).await {
        Ok(res) => res.choices.into_iter().next().and_then(|c| c.message.content).unwrap_or("Failed parsing text".to_string()),
        Err(e) => format!("LLM Error: {}", e)
    }
}
