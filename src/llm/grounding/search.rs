//! External search backend (Tavily-shaped). Provider-native web grounding needs
//! no code here — it is handled by the `web_search` plugin flag in transport.

use serde_json::{Value, json};

use crate::image_store;

/// Configuration for the search backend. When `external_key` is empty we rely on
/// the provider-native web plugin; otherwise we fetch snippets ourselves.
#[derive(Clone, Copy, Debug)]
pub struct SearchConfig<'a> {
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

/// Query a Tavily-shaped search API. Defensive: any error yields an empty vec so
/// grounding degrades to an ungrounded card rather than failing the request.
pub async fn search_external(
    cfg: &SearchConfig<'_>,
    query: &str,
    max_results: usize,
) -> Vec<SearchHit> {
    if cfg.external_key.trim().is_empty() || query.trim().is_empty() {
        return Vec::new();
    }

    let url = format!("{}/search", cfg.base_url.trim_end_matches('/'));
    let body = json!({
        "api_key": cfg.external_key,
        "query": query,
        "max_results": max_results,
    });

    let value = match image_store::post_public_json(&url, &body).await {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(error = ?err, "external search request failed safety validation");
            return Vec::new();
        }
    };

    value
        .get("results")
        .and_then(Value::as_array)
        .map(|results| {
            results
                .iter()
                .map(|hit| SearchHit {
                    title: string_field(hit, "title"),
                    url: string_field(hit, "url"),
                    snippet: string_field(hit, "content"),
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
