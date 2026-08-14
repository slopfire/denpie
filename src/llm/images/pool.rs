//! Pool image retrieval: ask the model to pick the best-matching image from the
//! user's uploaded library by id. The service loads the chosen row's bytes.

use serde_json::Value;

use crate::llm::transport::create_chat_completion;

use super::{ImageInput, RetrievedImage};

pub async fn retrieve(input: ImageInput<'_>) -> Option<RetrievedImage> {
    if input.pool.is_empty() {
        tracing::info!(
            topic = input.topic_name,
            card_title = input.card_title,
            has_api_key = !input.api_key.is_empty(),
            pool_images = input.pool.len(),
            "pool image strategy skipped because prerequisites are missing"
        );
        return None;
    }
    if input.pool.len() == 1 {
        let chosen_id = input.pool[0].id;
        tracing::info!(
            topic = input.topic_name,
            card_title = input.card_title,
            pool_id = chosen_id,
            "pool image strategy selected the only available image"
        );
        return Some(RetrievedImage::Pool(chosen_id));
    }
    if input.api_key.is_empty() {
        tracing::info!(
            topic = input.topic_name,
            card_title = input.card_title,
            "pool image strategy needs an API key to choose among multiple images"
        );
        return None;
    }

    tracing::info!(
        topic = input.topic_name,
        card_title = input.card_title,
        pool_images = input.pool.len(),
        "pool image strategy asking model to choose an image"
    );

    let catalog = input
        .pool
        .iter()
        .map(|img| {
            let description = img.description.as_deref().unwrap_or("");
            format!("- id {}: {} — {}", img.id, img.name, description)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "Pick the single best image to illustrate this tip card, or none if nothing fits.\n\
         Card title: {title}\n\
         Intended visual: {image_query}\n\
         Card content:\n{content}\n\n\
         Available images:\n{catalog}\n\n\
         Return JSON only: {{\"id\": <pool_id>}} for the best match, or {{\"id\": null}} if none fit.",
        title = input.card_title,
        image_query = input.image_query,
        content = input.card_content,
        catalog = catalog,
    );

    let response = create_chat_completion(
        input.model,
        &prompt,
        input.api_key,
        input.api_base,
        input.reasoning,
        Some(64),
    )
    .await;

    let Some(chosen_id) = parse_pool_choice(&response.content) else {
        tracing::info!(
            topic = input.topic_name,
            card_title = input.card_title,
            response_len = response.content.len(),
            "pool image strategy received no usable image choice"
        );
        return None;
    };
    if input.pool.iter().any(|img| img.id == chosen_id) {
        tracing::info!(
            topic = input.topic_name,
            card_title = input.card_title,
            pool_id = chosen_id,
            "pool image strategy selected an image"
        );
        Some(RetrievedImage::Pool(chosen_id))
    } else {
        tracing::warn!(
            topic = input.topic_name,
            card_title = input.card_title,
            pool_id = chosen_id,
            "pool image strategy selected an unknown image id"
        );
        None
    }
}

fn parse_pool_choice(content: &str) -> Option<i64> {
    let trimmed = content.trim();
    let json_text = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    let parsed = serde_json::from_str::<Value>(json_text).ok()?;
    parsed.get("id").and_then(Value::as_i64)
}

#[cfg(test)]
mod tests {
    use super::parse_pool_choice;

    #[test]
    fn parse_pool_choice_reads_id() {
        assert_eq!(parse_pool_choice("{\"id\": 7}"), Some(7));
    }

    #[test]
    fn parse_pool_choice_reads_fenced() {
        assert_eq!(parse_pool_choice("```json\n{\"id\": 3}\n```"), Some(3));
    }

    #[test]
    fn parse_pool_choice_null_is_none() {
        assert_eq!(parse_pool_choice("{\"id\": null}"), None);
    }
}
