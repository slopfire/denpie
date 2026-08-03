//! External search backends. Provider-native web grounding needs
//! no code here — it is handled by the `web_search` plugin flag in transport.

use serde_json::{Value, json};

use crate::domain::grounding::SearchProvider;
use crate::image_store;

/// Configuration for the search backend. When `external_key` is empty we rely on
/// the provider-native web plugin; otherwise we fetch snippets ourselves.
#[derive(Clone, Copy, Debug)]
pub struct SearchConfig<'a> {
    pub provider: &'a str,
    pub external_key: &'a str,
    pub base_url: &'a str,
}

impl SearchConfig<'_> {
    /// True when no external search key is configured, so grounding should use the
    /// provider-native web plugin instead of fetching snippets client-side.
    pub fn provider_native(&self) -> bool {
        self.external_key.trim().is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Query the configured search API. Defensive: any error yields an empty vec so
/// grounding degrades to an ungrounded card rather than failing the request.
pub async fn search_external(
    cfg: &SearchConfig<'_>,
    query: &str,
    max_results: usize,
) -> Vec<SearchHit> {
    if cfg.external_key.trim().is_empty() || query.trim().is_empty() {
        return Vec::new();
    }

    let provider = SearchProvider::from_setting(cfg.provider);
    let (url, body) = match provider {
        SearchProvider::Tavily => (
            format!("{}/search", cfg.base_url.trim_end_matches('/')),
            json!({
                "api_key": cfg.external_key,
                "query": query,
                "max_results": max_results,
            }),
        ),
        SearchProvider::Firecrawl => (
            format!("{}/v2/search", cfg.base_url.trim_end_matches('/')),
            json!({
                "query": query,
                "limit": max_results,
                "sources": ["web"],
                "scrapeOptions": {
                    "formats": [{"type": "markdown"}],
                    "onlyMainContent": true
                }
            }),
        ),
    };

    let response = match provider {
        SearchProvider::Tavily => image_store::post_public_json(&url, &body).await,
        SearchProvider::Firecrawl => {
            image_store::post_public_json_bearer(&url, &body, cfg.external_key).await
        }
    };
    let value = match response {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(error = ?err, "external search request failed safety validation");
            return Vec::new();
        }
    };

    parse_search_hits(provider, &value)
}

fn parse_search_hits(provider: SearchProvider, value: &Value) -> Vec<SearchHit> {
    let results = match provider {
        SearchProvider::Tavily => value.get("results"),
        SearchProvider::Firecrawl => value.get("data").and_then(|data| data.get("web")),
    };
    results
        .and_then(Value::as_array)
        .map(|results| {
            results
                .iter()
                .map(|hit| SearchHit {
                    title: string_field(hit, "title"),
                    url: string_field(hit, "url"),
                    snippet: match provider {
                        SearchProvider::Tavily => string_field(hit, "content"),
                        SearchProvider::Firecrawl => {
                            let markdown = string_field(hit, "markdown");
                            if markdown.is_empty() {
                                string_field(hit, "description")
                            } else {
                                markdown
                            }
                        }
                    },
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Render search hits into a context block for prompt injection.
pub fn render_hits(hits: &[SearchHit]) -> String {
    hits.iter()
        .enumerate()
        .map(|(idx, hit)| {
            format!(
                "[{}] {}\n{}\nSource: {}",
                idx + 1,
                hit.title,
                hit.snippet,
                hit.url
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_names_are_normalized() {
        assert_eq!(
            SearchProvider::from_setting("firecrawl"),
            SearchProvider::Firecrawl
        );
        assert_eq!(
            SearchProvider::from_setting("unknown"),
            SearchProvider::Tavily
        );
    }

    #[test]
    fn firecrawl_results_prefer_scraped_markdown() {
        let value = json!({
            "data": {"web": [{
                "title": "Guide",
                "url": "https://example.com/guide",
                "description": "Short result",
                "markdown": "# Full guide"
            }]}
        });
        let hits = parse_search_hits(SearchProvider::Firecrawl, &value);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].snippet, "# Full guide");
    }
}
