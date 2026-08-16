//! Keyless Bing Images discovery through the static search-result HTML.

use std::collections::HashSet;

use crate::image_store;

use super::{ImageInput, RetrievedImage, download_candidates};

const MAX_DISCOVERED_URLS: usize = 20;

pub async fn retrieve(input: ImageInput<'_>) -> Option<RetrievedImage> {
    if input.image_query.trim().is_empty() {
        return None;
    }
    let urls = match discover(input.image_query).await {
        Ok(urls) => urls,
        Err(error) => {
            tracing::warn!(%error, "Bing HTML image discovery failed");
            return None;
        }
    };
    download_candidates(&urls).await
}

pub async fn discover(query: &str) -> Result<Vec<String>, String> {
    let mut endpoint =
        url::Url::parse("https://www.bing.com/images/search").map_err(|error| error.to_string())?;
    endpoint
        .query_pairs_mut()
        .append_pair("q", query.trim())
        .append_pair("form", "HDRSC2")
        .append_pair("cc", "us")
        .append_pair("setlang", "en");
    let html = image_store::get_public_html(endpoint.as_str())
        .await
        .map_err(|(_, message)| message)?;
    if looks_like_consent_wall(&html) {
        return Err("Bing Images returned a consent wall instead of search results".to_string());
    }
    Ok(parse_image_urls(&html))
}

fn looks_like_consent_wall(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    let has_results = html.contains("\"murl\":\"")
        || html.contains("murl&quot;:&quot;")
        || html.contains("mediaurl=");
    !has_results
        && (lower.contains("consent")
            || lower.contains("cookie")
            || lower.contains("privacynotice"))
}

pub(crate) fn parse_image_urls(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut seen = HashSet::new();

    collect_json_murl(html, &mut urls, &mut seen);
    collect_html_murl(html, &mut urls, &mut seen);
    collect_mediaurl(html, &mut urls, &mut seen);
    urls.truncate(MAX_DISCOVERED_URLS);
    urls
}

fn collect_json_murl(html: &str, urls: &mut Vec<String>, seen: &mut HashSet<String>) {
    const PREFIX: &str = "\"murl\":\"";
    let mut rest = html;
    while let Some(start) = rest.find(PREFIX) {
        rest = &rest[start + PREFIX.len()..];
        let Some(end) = json_string_end(rest) else {
            break;
        };
        let encoded = &rest[..end];
        if let Ok(url) = serde_json::from_str::<String>(&format!("\"{encoded}\"")) {
            push_url(url, urls, seen);
        }
        rest = &rest[end + 1..];
    }
}

fn json_string_end(value: &str) -> Option<usize> {
    let mut escaped = false;
    for (index, byte) in value.bytes().enumerate() {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'\"' {
            return Some(index);
        }
    }
    None
}

fn collect_html_murl(html: &str, urls: &mut Vec<String>, seen: &mut HashSet<String>) {
    const PREFIX: &str = "murl&quot;:&quot;";
    let mut rest = html;
    while let Some(start) = rest.find(PREFIX) {
        rest = &rest[start + PREFIX.len()..];
        let Some(end) = rest.find("&quot;") else {
            break;
        };
        push_url(decode_html_entities(&rest[..end]), urls, seen);
        rest = &rest[end + "&quot;".len()..];
    }
}

fn collect_mediaurl(html: &str, urls: &mut Vec<String>, seen: &mut HashSet<String>) {
    const PREFIX: &str = "mediaurl=";
    let mut rest = html;
    while let Some(start) = rest.find(PREFIX) {
        rest = &rest[start + PREFIX.len()..];
        let end = rest
            .find(|character: char| {
                character == '&'
                    || character == '\"'
                    || character == '\''
                    || character.is_whitespace()
            })
            .unwrap_or(rest.len());
        let encoded = &rest[..end];
        let query = format!("url={encoded}");
        if let Some((_, decoded)) = url::form_urlencoded::parse(query.as_bytes()).next() {
            push_url(decoded.into_owned(), urls, seen);
        }
        rest = &rest[end..];
    }
}

pub(crate) fn decode_html_entities(value: &str) -> String {
    value
        .replace("&#x2F;", "/")
        .replace("&#47;", "/")
        .replace("&#34;", "\"")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

fn push_url(url: String, urls: &mut Vec<String>, seen: &mut HashSet<String>) {
    let url = decode_html_entities(url.trim());
    let valid = url::Url::parse(&url).ok().is_some_and(|parsed| {
        matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some()
    });
    if valid && seen.insert(url.clone()) && urls.len() < MAX_DISCOVERED_URLS {
        urls.push(url);
    }
}

#[cfg(test)]
mod tests {
    use super::parse_image_urls;

    #[test]
    fn parser_reads_all_bing_encodings_in_rank_order_without_duplicates() {
        let urls = parse_image_urls(include_str!("fixtures/bing_results.html"));
        assert_eq!(
            urls,
            vec![
                "https://images.example/first diagram.png?x=1&y=2",
                "https://images.example/second.jpg?a=1&b=2",
                "https://images.example/third.webp",
            ]
        );
    }

    #[test]
    fn consent_or_empty_html_returns_no_candidates() {
        assert!(parse_image_urls("<html><title>Consent</title></html>").is_empty());
    }

    #[test]
    fn consent_wall_is_detected_when_no_image_metadata_is_present() {
        assert!(super::looks_like_consent_wall(
            "<html><title>Consent</title><p>cookie settings</p></html>"
        ));
        assert!(!super::looks_like_consent_wall(
            r#"<html><script>{"murl":"https://images.example/a.png"}</script></html>"#
        ));
    }

    #[tokio::test]
    #[ignore = "live Bing smoke; not for CI"]
    async fn live_bing_discovers_image_candidates() {
        let urls = super::discover("diagram of in on at prepositions of place")
            .await
            .unwrap();
        assert!(!urls.is_empty());
        assert!(super::download_candidates(&urls).await.is_some());
    }
}
