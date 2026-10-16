//! External search backends. Provider-native web grounding needs no code here —
//! it is handled by the `web_search` plugin flag in transport.

use serde_json::{Value, json};

use crate::domain::grounding::{Freshness, GroundingSearchProfile, SearchDepth, SearchProvider};
use crate::image_store;

/// Configuration for the configured external search backend.
#[derive(Clone, Debug)]
pub struct SearchConfig<'a> {
    pub provider: &'a str,
    pub external_key: &'a str,
    pub base_url: &'a str,
    pub profile: &'a GroundingSearchProfile,
}

impl SearchConfig<'_> {
    /// Whether the user explicitly selected LLM-provider-native web search.
    ///
    /// An absent external key is a configuration error, not an implicit switch
    /// to native search: the latter cannot honour this profile's domain and
    /// freshness constraints.
    pub fn provider_native(&self) -> bool {
        SearchProvider::from_setting(self.provider) == SearchProvider::Native
    }
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Query the configured external search API with the topic's search profile.
///
/// Search is evidence acquisition, so failures are returned to the grounding
/// policy rather than being silently treated as an empty source list.
pub async fn search_external(
    cfg: &SearchConfig<'_>,
    query: &str,
) -> Result<Vec<SearchHit>, String> {
    if cfg.provider_native() {
        return Err("native search is not an external search backend".to_string());
    }
    if cfg.external_key.trim().is_empty() {
        return Err("external search API key is not configured".to_string());
    }
    if query.trim().is_empty() {
        return Err("search query is empty".to_string());
    }

    let provider = SearchProvider::from_setting(cfg.provider);
    let (url, body) = external_request(cfg, provider, query);

    let response = match provider {
        SearchProvider::Tavily => image_store::post_public_json(&url, &body).await,
        SearchProvider::Firecrawl => {
            image_store::post_public_json_bearer(&url, &body, cfg.external_key).await
        }
        SearchProvider::Native => unreachable!("native provider returned above"),
    };
    let value = response.map_err(|(status, message)| {
        format!(
            "{} search request failed ({status}): {message}",
            provider.as_str()
        )
    })?;

    parse_search_hits(provider, &value)
}

fn external_request(
    cfg: &SearchConfig<'_>,
    provider: SearchProvider,
    query: &str,
) -> (String, Value) {
    let query = query_with_guidance(query, &cfg.profile.guidance);
    let limit = cfg.profile.depth.result_limit();
    let (url, mut body) = match provider {
        SearchProvider::Tavily => (
            format!("{}/search", cfg.base_url.trim_end_matches('/')),
            json!({
                "api_key": cfg.external_key,
                "query": query,
                "max_results": limit,
                "search_depth": tavily_search_depth(cfg.profile.depth, cfg.profile.freshness),
            }),
        ),
        SearchProvider::Firecrawl => (
            format!("{}/v2/search", cfg.base_url.trim_end_matches('/')),
            json!({
                "query": query,
                "limit": limit,
                "sources": ["web"],
                "scrapeOptions": {
                    "formats": [{"type": "markdown"}],
                    "onlyMainContent": true
                }
            }),
        ),
        SearchProvider::Native => unreachable!("native provider cannot build an external request"),
    };

    let object = body
        .as_object_mut()
        .expect("external search request body is always an object");
    if let Some(time_range) = tavily_time_range(cfg.profile.freshness) {
        match provider {
            SearchProvider::Tavily => {
                object.insert("time_range".to_string(), json!(time_range));
            }
            SearchProvider::Firecrawl => {
                object.insert(
                    "tbs".to_string(),
                    json!(firecrawl_tbs(cfg.profile.freshness)),
                );
            }
            SearchProvider::Native => {
                unreachable!("native provider cannot build an external request")
            }
        }
    }
    if !cfg.profile.include_domains.is_empty() {
        let domains = json!(cfg.profile.include_domains);
        match provider {
            SearchProvider::Tavily => {
                object.insert("include_domains".to_string(), domains);
            }
            SearchProvider::Firecrawl => {
                object.insert("includeDomains".to_string(), domains);
            }
            SearchProvider::Native => {
                unreachable!("native provider cannot build an external request")
            }
        }
    }

    (url, body)
}

fn query_with_guidance(query: &str, guidance: &str) -> String {
    let guidance = guidance.trim();
    if guidance.is_empty() {
        query.trim().to_string()
    } else {
        format!("{}\n\nResearch guidance: {guidance}", query.trim())
    }
}

fn tavily_search_depth(depth: SearchDepth, freshness: Freshness) -> &'static str {
    if depth == SearchDepth::Deep || freshness != Freshness::Any {
        "advanced"
    } else {
        "basic"
    }
}

fn tavily_time_range(freshness: Freshness) -> Option<&'static str> {
    match freshness {
        Freshness::Any => None,
        Freshness::Week => Some("week"),
        Freshness::Month => Some("month"),
        Freshness::Year => Some("year"),
    }
}

fn firecrawl_tbs(freshness: Freshness) -> &'static str {
    match freshness {
        Freshness::Any => unreachable!("any freshness has no Firecrawl tbs filter"),
        Freshness::Week => "qdr:w",
        Freshness::Month => "qdr:m",
        Freshness::Year => "qdr:y",
    }
}

fn parse_search_hits(provider: SearchProvider, value: &Value) -> Result<Vec<SearchHit>, String> {
    let results = match provider {
        SearchProvider::Tavily => value.get("results"),
        SearchProvider::Firecrawl => value.get("data").and_then(|data| data.get("web")),
        SearchProvider::Native => {
            return Err("native search does not return external hits".to_string());
        }
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
                        SearchProvider::Native => {
                            unreachable!("native search does not return external hits")
                        }
                    },
                })
                .collect()
        })
        .ok_or_else(|| {
            format!(
                "{} search response did not contain a result array",
                provider.as_str()
            )
        })
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
            SearchProvider::from_setting("native"),
            SearchProvider::Native
        );
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
        let hits = parse_search_hits(SearchProvider::Firecrawl, &value).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].snippet, "# Full guide");
    }

    #[test]
    fn tavily_request_uses_every_profile_control() {
        let profile = GroundingSearchProfile {
            depth: SearchDepth::Deep,
            freshness: Freshness::Month,
            guidance: "Prefer official documentation".to_string(),
            include_domains: vec!["docs.example.com".to_string()],
            ..GroundingSearchProfile::default()
        };
        let cfg = SearchConfig {
            provider: "tavily",
            external_key: "key",
            base_url: "https://api.tavily.com/",
            profile: &profile,
        };

        let (url, body) = external_request(&cfg, SearchProvider::Tavily, "Rust releases");

        assert_eq!(url, "https://api.tavily.com/search");
        assert_eq!(
            body["query"],
            "Rust releases\n\nResearch guidance: Prefer official documentation"
        );
        assert_eq!(body["max_results"], 12);
        assert_eq!(body["search_depth"], "advanced");
        assert_eq!(body["time_range"], "month");
        assert_eq!(body["include_domains"], json!(["docs.example.com"]));
    }

    #[test]
    fn firecrawl_request_uses_provider_specific_filters() {
        let profile = GroundingSearchProfile {
            depth: SearchDepth::Quick,
            freshness: Freshness::Week,
            include_domains: vec!["example.com".to_string()],
            ..GroundingSearchProfile::default()
        };
        let cfg = SearchConfig {
            provider: "firecrawl",
            external_key: "key",
            base_url: "https://api.firecrawl.dev",
            profile: &profile,
        };

        let (url, body) = external_request(&cfg, SearchProvider::Firecrawl, "current news");

        assert_eq!(url, "https://api.firecrawl.dev/v2/search");
        assert_eq!(body["limit"], 3);
        assert_eq!(body["tbs"], "qdr:w");
        assert_eq!(body["includeDomains"], json!(["example.com"]));
    }

    #[test]
    fn native_is_explicit_and_an_empty_key_is_not_native() {
        let profile = GroundingSearchProfile::default();
        let native = SearchConfig {
            provider: "native",
            external_key: "",
            base_url: "",
            profile: &profile,
        };
        let external_without_key = SearchConfig {
            provider: "tavily",
            external_key: "",
            base_url: "https://api.tavily.com",
            profile: &profile,
        };

        assert!(native.provider_native());
        assert!(!external_without_key.provider_native());
    }

    #[test]
    fn tavily_uses_advanced_search_when_a_time_filter_needs_it() {
        assert_eq!(
            tavily_search_depth(SearchDepth::Quick, Freshness::Week),
            "advanced"
        );
        assert_eq!(
            tavily_search_depth(SearchDepth::Quick, Freshness::Any),
            "basic"
        );
    }

    #[test]
    fn profile_depth_sets_quick_balanced_and_deep_result_limits() {
        assert_eq!(SearchDepth::Quick.result_limit(), 3);
        assert_eq!(SearchDepth::Balanced.result_limit(), 6);
        assert_eq!(SearchDepth::Deep.result_limit(), 12);
    }

    #[test]
    fn malformed_provider_response_is_an_error() {
        let error = parse_search_hits(SearchProvider::Tavily, &json!({})).unwrap_err();

        assert_eq!(
            error,
            "tavily search response did not contain a result array"
        );
    }
}
