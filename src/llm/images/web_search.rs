//! Tavily-compatible image search. Search result URLs are treated as untrusted;
//! `image_store::download_remote_image` performs the SSRF and redirect checks.

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;

use crate::image_store::{self, IncomingImage};

use super::{ImageInput, RetrievedImage, download_and_prepare};

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    images: Vec<ImageResult>,
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
    retrieve_with_domains(&input, &[]).await
}

pub(super) async fn retrieve_with_domains(
    input: &ImageInput<'_>,
    allowed_domains: &[String],
) -> Option<RetrievedImage> {
    if input.search_api_key.trim().is_empty() || input.image_query.trim().is_empty() {
        return None;
    }
    let endpoint = format!("{}/search", input.search_base_url.trim_end_matches('/'));
    let body = search_request_body(input.search_api_key, input.image_query, allowed_domains);
    let value = image_store::post_public_json(&endpoint, &body).await.ok()?;
    let results: SearchResponse = serde_json::from_value(value).ok()?;
    for result in results.images {
        if !allowed_domains.is_empty() {
            if let Some(image) = download_and_prepare(result.url(), allowed_domains).await {
                return Some(image);
            }
            continue;
        }
        let Ok(incoming) = image_store::download_remote_image(result.url()).await else {
            continue;
        };
        if let Some(data_url) = incoming_data_url(incoming) {
            return Some(RetrievedImage {
                data_url,
                pool_id: None,
            });
        }
    }
    None
}

fn search_request_body(
    api_key: &str,
    query: &str,
    allowed_domains: &[String],
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "api_key": api_key,
        "query": query,
        "max_results": 5,
        "include_images": true,
        "include_image_descriptions": true,
    });
    if !allowed_domains.is_empty() {
        body["include_domains"] = serde_json::json!(allowed_domains);
    }
    body
}

fn incoming_data_url(incoming: IncomingImage) -> Option<String> {
    match incoming {
        IncomingImage::Bytes { bytes, mime_type } => Some(format!(
            "data:{mime_type};base64,{}",
            STANDARD.encode(bytes)
        )),
        IncomingImage::DataUrl(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchResponse, search_request_body};

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
        let body = search_request_body("key", "cell anatomy diagram", &[]);
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
        let body = search_request_body("key", "cell anatomy diagram", &domains);
        assert_eq!(
            body["include_domains"],
            serde_json::json!(["docs.example.com", "cdn.example.com"])
        );
    }
}
