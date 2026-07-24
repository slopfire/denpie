//! Isolated image search: query the configured web-search API for images only
//! inside each enabled source card's allowed domains.

use crate::domain::grounding::ImageSourceKind;

use super::{ImageInput, RetrievedImage, web_search};

pub async fn retrieve(input: ImageInput<'_>) -> Option<RetrievedImage> {
    let has_sources = input.sources.iter().any(|source| {
        source.enabled
            && source.kind == ImageSourceKind::WebSearch
            && !source.download_hosts().is_empty()
    });
    if input.search_api_key.trim().is_empty() || !has_sources {
        tracing::info!(
            topic = input.topic_name,
            card_title = input.card_title,
            has_search_api_key = !input.search_api_key.trim().is_empty(),
            has_sources,
            "isolated image search skipped because prerequisites are missing"
        );
        return None;
    }

    for source in input
        .sources
        .iter()
        .filter(|source| source.enabled && source.kind == ImageSourceKind::WebSearch)
    {
        let download_hosts = source.download_hosts();
        if download_hosts.is_empty() {
            tracing::warn!(
                source_id = source.id,
                "web image source has no download hosts"
            );
            continue;
        }
        tracing::info!(
            topic = input.topic_name,
            card_title = input.card_title,
            source_id = source.id,
            source_name = source.name,
            allowed_hosts = download_hosts.len(),
            "isolated image search querying allowed domains"
        );
        if let Some(image) = web_search::retrieve_with_domains(&input, &download_hosts).await {
            return Some(image);
        }
    }

    None
}
