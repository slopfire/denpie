//! Configured image API retrieval: the model supplies only source-specific
//! tags; endpoint, query parameter, JSON path, and host policy come from a source card.

use crate::domain::grounding::{ImageSource, ImageSourceKind};
use serde_json::Value;

use crate::image_store;
use crate::llm::transport::create_chat_completion;

use super::{ImageInput, RetrievedImage, download_and_prepare, host_allowed};

pub async fn retrieve(input: ImageInput<'_>) -> Option<RetrievedImage> {
    let has_sources = input
        .sources
        .iter()
        .any(|source| source.enabled && source.kind == ImageSourceKind::Api);
    if input.api_key.is_empty() || !has_sources {
        tracing::info!(
            topic = input.topic_name,
            card_title = input.card_title,
            has_api_key = !input.api_key.is_empty(),
            has_sources,
            "image API strategy skipped because prerequisites are missing"
        );
        return None;
    }

    for source in input
        .sources
        .iter()
        .filter(|source| source.enabled && source.kind == ImageSourceKind::Api)
    {
        tracing::info!(
            topic = input.topic_name,
            card_title = input.card_title,
            source_id = source.id,
            source_name = source.name,
            "image API source asking model for tags"
        );
        let prompt = format!(
            "Generate search tags for one configured image source.\n\
             Card title: {title}\n\
             Intended visual: {image_query}\n\
             Topic: {topic}\n\
             Source: {source_name}\n\
             Fixed tags already applied: {default_tags}\n\
             Source instructions: {instructions}\n\n\
             Return JSON only: {{\"tags\": \"<additional source-specific tags>\"}}.\n\
             Do not return a URL, endpoint, method, or JSON path.",
            title = input.card_title,
            image_query = input.image_query,
            topic = input.topic_name,
            source_name = source.name,
            default_tags = source.default_tags,
            instructions = source.instructions,
        );
        let response = create_chat_completion(
            input.model,
            &prompt,
            input.api_key,
            input.api_base,
            input.reasoning,
            Some(128),
        )
        .await;

        let Some(tags) = parse_tags(&response.content) else {
            tracing::warn!(
                source_id = source.id,
                response_len = response.content.len(),
                "image API source response contained no usable tags"
            );
            continue;
        };
        let Some(recipe_url) = build_request_url(source, &tags) else {
            tracing::warn!(
                source_id = source.id,
                "image API source configuration is invalid"
            );
            continue;
        };
        tracing::info!(
            source_id = source.id,
            recipe_url,
            "image API source resolving configured endpoint"
        );

        let image_url = if source.json_path.trim().is_empty() {
            recipe_url
        } else {
            let Some(image_url) = resolve_via_json(&recipe_url, source.json_path.trim()).await
            else {
                tracing::warn!(
                    source_id = source.id,
                    json_path = source.json_path,
                    "image API source could not resolve configured JSON path"
                );
                continue;
            };
            image_url
        };
        let download_hosts = source.download_hosts();
        if let Some(image) = download_and_prepare(&image_url, &download_hosts).await {
            tracing::info!(
                source_id = source.id,
                image_url,
                "image API source downloaded an image"
            );
            return Some(image);
        }
    }

    None
}

fn parse_tags(content: &str) -> Option<String> {
    let trimmed = content.trim();
    let json_text = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    serde_json::from_str::<Value>(json_text)
        .ok()?
        .get("tags")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn build_request_url(source: &ImageSource, generated_tags: &str) -> Option<String> {
    let api_hosts = source.api_hosts();
    if !host_allowed(&source.endpoint, &api_hosts) {
        return None;
    }
    let mut url = url::Url::parse(&source.endpoint).ok()?;
    let query_parameter = source.query_parameter.trim();
    if !query_parameter.is_empty() {
        let tags = [source.default_tags.trim(), generated_tags.trim()]
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !tags.is_empty() {
            url.query_pairs_mut().append_pair(query_parameter, &tags);
        }
    }
    Some(url.into())
}

/// Fetch a JSON document from an allowed host and extract a string at a dotted
/// or single-key path (e.g. `file_url`). Returns the URL string if found.
async fn resolve_via_json(url: &str, json_path: &str) -> Option<String> {
    let value = match image_store::get_public_json(url).await {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(error = ?err, "recipe json fetch failed safety validation");
            return None;
        }
    };

    let mut current = &value;
    for segment in json_path.split('.') {
        current = match current {
            // Some APIs (e.g. random.json arrays) return a list — index into the first element.
            Value::Array(items) => items.first()?,
            _ => current,
        };
        current = current.get(segment)?;
    }
    if let Value::Array(items) = current {
        current = items.first()?;
    }
    current.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{build_request_url, parse_tags};
    use crate::domain::grounding::{ImageSource, ImageSourceKind};

    fn source() -> ImageSource {
        ImageSource {
            id: "danbooru".to_string(),
            name: "Danbooru".to_string(),
            kind: ImageSourceKind::Api,
            enabled: true,
            endpoint: "https://danbooru.donmai.us/posts/random.json".to_string(),
            query_parameter: "tags".to_string(),
            json_path: "file_url".to_string(),
            default_tags: "rating:general".to_string(),
            api_hosts: "danbooru.donmai.us".to_string(),
            search_domains: String::new(),
            download_hosts: "cdn.donmai.us".to_string(),
            instructions: String::new(),
        }
    }

    #[test]
    fn parse_tags_reads_json() {
        assert_eq!(
            parse_tags("{\"tags\":\"helix_editor keyboard\"}"),
            Some("helix_editor keyboard".to_string())
        );
    }

    #[test]
    fn configured_request_combines_and_encodes_tags() {
        let url = build_request_url(&source(), "helix_editor keyboard").unwrap();
        assert_eq!(
            url,
            "https://danbooru.donmai.us/posts/random.json?tags=rating%3Ageneral+helix_editor+keyboard"
        );
    }

    #[test]
    fn configured_request_rejects_unlisted_api_host() {
        let mut source = source();
        source.endpoint = "https://evil.example/posts.json".to_string();
        assert!(build_request_url(&source, "helix").is_none());
    }
}
