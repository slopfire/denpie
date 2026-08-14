//! External-provider image search. Search result URLs are treated as untrusted;
//! `image_store::download_remote_image` performs the SSRF and redirect checks.

use serde::Deserialize;

use crate::domain::grounding::SearchProvider;
use crate::image_store;

use super::{ImageInput, RetrievedImage, download_and_prepare};

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    images: Vec<ImageResult>,
}

#[derive(Deserialize)]
struct FirecrawlSearchResponse {
    #[serde(default)]
    data: FirecrawlSearchData,
}

#[derive(Default, Deserialize)]
struct FirecrawlSearchData {
    #[serde(default)]
    images: Vec<FirecrawlImageResult>,
}

#[derive(Deserialize)]
struct FirecrawlImageResult {
    #[serde(rename = "imageUrl")]
    image_url: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ImageResult {
    Url(String),
    Detailed {
        url: String,
        #[allow(dead_code)]
        description: Option<String>,
    },
}

impl ImageResult {
    fn url(&self) -> &str {
        match self {
            Self::Url(url) | Self::Detailed { url, .. } => url,
        }
    }
}

pub async fn retrieve(input: ImageInput<'_>) -> Option<RetrievedImage> {
    retrieve_with_policy(&input, &[], &[], "").await
}

pub(super) async fn retrieve_with_policy(
    input: &ImageInput<'_>,
    search_domains: &[String],
    download_hosts: &[String],
    instructions: &str,
) -> Option<RetrievedImage> {
    if input.search_api_key.trim().is_empty() || input.image_query.trim().is_empty() {
        return None;
    }
    let provider = SearchProvider::from_setting(input.search_provider);
    let endpoint = match provider {
        SearchProvider::Tavily => format!("{}/search", input.search_base_url.trim_end_matches('/')),
        SearchProvider::Firecrawl => {
            format!("{}/v2/search", input.search_base_url.trim_end_matches('/'))
        }
    };
    let query = provider_query(input.image_query, instructions);
    let body = search_request_body(provider, input.search_api_key, &query, search_domains);
    let value = match provider {
        SearchProvider::Tavily => image_store::post_public_json(&endpoint, &body).await.ok()?,
        SearchProvider::Firecrawl => {
            image_store::post_public_json_bearer(&endpoint, &body, input.search_api_key)
                .await
                .ok()?
        }
    };
    let urls: Vec<String> = match provider {
        SearchProvider::Tavily => serde_json::from_value::<SearchResponse>(value)
            .ok()?
            .images
            .into_iter()
            .map(|result| result.url().to_string())
            .collect(),
        SearchProvider::Firecrawl => serde_json::from_value::<FirecrawlSearchResponse>(value)
            .ok()?
            .data
            .images
            .into_iter()
            .map(|result| result.image_url)
            .collect(),
    };
    for url in urls {
        if let Some(image) = download_and_prepare(&url, download_hosts).await {
            return Some(image);
        }
    }
    None
}

fn provider_query(query: &str, instructions: &str) -> String {
    if instructions.trim().is_empty() {
        query.trim().to_string()
    } else {
        format!("{}. {}", query.trim(), instructions.trim())
    }
}

fn search_request_body(
    provider: SearchProvider,
    api_key: &str,
    query: &str,
    allowed_domains: &[String],
) -> serde_json::Value {
    let mut body = match provider {
        SearchProvider::Tavily => serde_json::json!({
            "api_key": api_key,
            "query": query,
            "max_results": 5,
            "include_images": true,
            "include_image_descriptions": true,
        }),
        SearchProvider::Firecrawl => serde_json::json!({
            "query": query,
            "limit": 5,
            "sources": ["images"]
        }),
    };
    if !allowed_domains.is_empty() {
        let key = match provider {
            SearchProvider::Tavily => "include_domains",
            SearchProvider::Firecrawl => "includeDomains",
        };
        body[key] = serde_json::json!(allowed_domains);
    }
    body
}

#[cfg(test)]
mod tests {
    use super::{FirecrawlSearchResponse, SearchResponse, provider_query, search_request_body};
    use crate::domain::grounding::SearchProvider;

    #[test]
    fn parses_string_and_described_image_results() {
        let parsed: SearchResponse = serde_json::from_str(
            r#"{"images":["https://a.example/a.png",{"url":"https://b.example/b.png","description":"B"}]}"#,
        ).unwrap();
        assert_eq!(parsed.images.len(), 2);
        assert_eq!(parsed.images[0].url(), "https://a.example/a.png");
        assert_eq!(parsed.images[1].url(), "https://b.example/b.png");
    }

    #[test]
    fn request_matches_tavily_image_search_contract() {
        let body = search_request_body(SearchProvider::Tavily, "key", "cell anatomy diagram", &[]);
        assert_eq!(body["api_key"], "key");
        assert_eq!(body["query"], "cell anatomy diagram");
        assert_eq!(body["max_results"], 5);
        assert_eq!(body["include_images"], true);
        assert_eq!(body["include_image_descriptions"], true);
        assert!(body.get("include_domains").is_none());
    }

    #[test]
    fn request_can_limit_image_search_to_allowed_domains() {
        let domains = vec![
            "docs.example.com".to_string(),
            "cdn.example.com".to_string(),
        ];
        let body = search_request_body(
            SearchProvider::Tavily,
            "key",
            "cell anatomy diagram",
            &domains,
        );
        assert_eq!(
            body["include_domains"],
            serde_json::json!(["docs.example.com", "cdn.example.com"])
        );
    }

    #[test]
    fn firecrawl_request_and_response_match_image_contract() {
        let body = search_request_body(SearchProvider::Firecrawl, "ignored", "cell anatomy", &[]);
        assert_eq!(body["sources"], serde_json::json!(["images"]));
        assert!(body.get("api_key").is_none());
        let response: FirecrawlSearchResponse =
            serde_json::from_str(r#"{"data":{"images":[{"imageUrl":"https://a.example/a.png"}]}}"#)
                .unwrap();
        assert_eq!(response.data.images[0].image_url, "https://a.example/a.png");
    }

    #[test]
    fn source_instructions_are_included_in_provider_query() {
        assert_eq!(
            provider_query("borrow checker diagram", "Avoid logos and placeholders."),
            "borrow checker diagram. Avoid logos and placeholders."
        );
    }
}
