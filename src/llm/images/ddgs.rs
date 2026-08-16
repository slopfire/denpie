//! Optional DDGS text-search sidecar for discovering page/image URLs.
//!
//! The sidecar never downloads response bodies.  It only returns URLs; callers
//! must pass those URLs through the shared image downloader before persistence.

use std::{
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

use crate::image_store;

const DDGS_TIMEOUT: Duration = Duration::from_secs(15);
const STDOUT_CAP: usize = 64 * 1024;
const MAX_RESULTS: usize = 8;
const HELPER_NAME: &str = "scripts/ddgs-image-search.py";

/// Discover DDGS candidates and hand them to the shared downloader.
pub async fn retrieve(input: super::ImageInput<'_>) -> Option<super::RetrievedImage> {
    if input.image_query.trim().is_empty() {
        return None;
    }
    let pages = match discover_pages(input.image_query).await {
        Ok(urls) => urls,
        Err(error) => {
            tracing::warn!(%error, "DDGS image discovery failed");
            return None;
        }
    };
    let urls = tokio::time::timeout(Duration::from_secs(15), resolve_image_urls(&pages))
        .await
        .unwrap_or_else(|_| {
            tracing::warn!("DDGS Open Graph resolution exceeded the total deadline");
            Vec::new()
        });
    super::download_candidates(&urls).await
}

async fn resolve_image_urls(pages: &[String]) -> Vec<String> {
    let mut images = Vec::new();
    for page in pages.iter().take(3) {
        if looks_like_direct_image(page) {
            images.push(page.clone());
            continue;
        }
        let html = match image_store::get_public_html(page).await {
            Ok(html) => html,
            Err((_, error)) => {
                tracing::debug!(host = url_host(page), %error, "DDGS result page could not be fetched");
                continue;
            }
        };
        if let Some(image) = extract_page_image(&html, page) {
            images.push(image);
        }
    }
    images
}

fn looks_like_direct_image(value: &str) -> bool {
    url::Url::parse(value).ok().is_some_and(|url| {
        let path = url.path().to_ascii_lowercase();
        [".png", ".jpg", ".jpeg", ".webp", ".gif"]
            .iter()
            .any(|extension| path.ends_with(extension))
    })
}

fn extract_page_image(html: &str, page_url: &str) -> Option<String> {
    for tag in html_tags(html) {
        let attributes = parse_attributes(tag);
        let property = attribute(&attributes, "property")
            .or_else(|| attribute(&attributes, "name"))
            .unwrap_or_default();
        let relation = attribute(&attributes, "rel").unwrap_or_default();
        let candidate = if property.eq_ignore_ascii_case("og:image")
            || property.eq_ignore_ascii_case("og:image:url")
        {
            attribute(&attributes, "content")
        } else if relation
            .split_ascii_whitespace()
            .any(|value| value.eq_ignore_ascii_case("image_src"))
        {
            attribute(&attributes, "href")
        } else {
            None
        };
        let Some(candidate) = candidate else {
            continue;
        };
        let candidate = super::bing::decode_html_entities(candidate);
        if candidate.trim_start().starts_with("data:") {
            continue;
        }
        let base = url::Url::parse(page_url).ok()?;
        let resolved = base.join(candidate.trim()).ok()?;
        if matches!(resolved.scheme(), "http" | "https") {
            return Some(resolved.to_string());
        }
    }
    None
}

fn html_tags(html: &str) -> impl Iterator<Item = &str> {
    html.split('<').filter_map(|fragment| {
        let lower = fragment
            .get(..fragment.len().min(5))
            .unwrap_or(fragment)
            .to_ascii_lowercase();
        if !lower.starts_with("meta") && !lower.starts_with("link") {
            return None;
        }
        fragment.find('>').map(|end| &fragment[..end])
    })
}

fn parse_attributes(tag: &str) -> Vec<(String, String)> {
    let bytes = tag.as_bytes();
    let mut attributes = Vec::new();
    let mut index = tag.find(char::is_whitespace).unwrap_or(tag.len());
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let name_start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        if name_start == index {
            index += 1;
            continue;
        }
        let name = tag[name_start..index]
            .trim_end_matches('/')
            .to_ascii_lowercase();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let (value_start, value_end) =
            if index < bytes.len() && matches!(bytes[index], b'\'' | b'\"') {
                let quote = bytes[index];
                index += 1;
                let start = index;
                while index < bytes.len() && bytes[index] != quote {
                    index += 1;
                }
                let end = index;
                index = index.saturating_add(1);
                (start, end)
            } else {
                let start = index;
                while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                (start, index)
            };
        attributes.push((name, tag[value_start..value_end].to_string()));
    }
    attributes
}

fn attribute<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.as_str())
}

fn url_host(value: &str) -> String {
    url::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_default()
}

/// Discover page or direct-image URLs with the optional DDGS Python helper.
///
/// A missing helper/interpreter, a disabled sidecar, a timeout, malformed JSON,
/// or a non-zero sidecar exit is returned as an error so an image strategy can
/// treat it as a normal fallback miss.
pub async fn discover_pages(query: &str) -> Result<Vec<String>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("DDGS query is empty".to_string());
    }
    if disabled_by_env() {
        return Err("DDGS disabled by DENPIE_DISABLE_DDGS".to_string());
    }

    let (binary, args) = command_spec(&search_query(query));
    let mut child = Command::new(&binary)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // Do not let an untrusted helper fill a stderr pipe and deadlock us.
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| format!("failed to start DDGS sidecar ({binary:?}): {err}"))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "DDGS sidecar stdout was not piped".to_string())?;
    let mut bytes = Vec::new();
    let deadline = Instant::now() + DDGS_TIMEOUT;
    let read_result = tokio::time::timeout(
        DDGS_TIMEOUT,
        read_capped(&mut stdout, &mut bytes, STDOUT_CAP),
    )
    .await;
    match read_result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(format!("failed to read DDGS sidecar output: {err}"));
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(format!(
                "DDGS sidecar timed out after {} seconds",
                DDGS_TIMEOUT.as_secs()
            ));
        }
    }

    let remaining = deadline.saturating_duration_since(Instant::now());
    let status = match tokio::time::timeout(remaining, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(err)) => return Err(format!("failed waiting for DDGS sidecar: {err}")),
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(format!(
                "DDGS sidecar timed out after {} seconds",
                DDGS_TIMEOUT.as_secs()
            ));
        }
    };
    if !status.success() {
        return Err(format!("DDGS sidecar exited with status {status}"));
    }

    parse_output(&bytes)
}

async fn read_capped<R: AsyncRead + Unpin>(
    reader: &mut R,
    bytes: &mut Vec<u8>,
    cap: usize,
) -> std::io::Result<()> {
    // Reading one byte beyond the cap lets us reject oversized output while
    // still promptly returning and killing a helper that ignores the contract.
    reader.take((cap + 1) as u64).read_to_end(bytes).await?;
    if bytes.len() > cap {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "DDGS sidecar stdout exceeded the output cap",
        ));
    }
    Ok(())
}

fn parse_output(bytes: &[u8]) -> Result<Vec<String>, String> {
    if bytes.len() > STDOUT_CAP {
        return Err(format!(
            "DDGS sidecar stdout exceeded the {STDOUT_CAP}-byte limit"
        ));
    }
    let values: Vec<String> = serde_json::from_slice(bytes)
        .map_err(|err| format!("DDGS sidecar returned invalid JSON: {err}"))?;
    let mut urls = Vec::with_capacity(values.len().min(MAX_RESULTS));
    for value in values {
        let value = value.trim();
        if !matches!(url::Url::parse(value), Ok(url) if matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        {
            continue;
        }
        if !urls.iter().any(|url| url == value) {
            urls.push(value.to_string());
        }
        if urls.len() == MAX_RESULTS {
            break;
        }
    }
    Ok(urls)
}

fn command_spec(query: &str) -> (PathBuf, Vec<String>) {
    let configured = std::env::var_os("DENPIE_DDGS_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("python3"));
    // The documented override is a Python executable.  Accepting a .py path
    // too is useful for virtualenv/development checks and keeps the argv shape
    // unambiguous for callers that package the helper separately.
    if configured.extension().is_some_and(|ext| ext == "py") {
        return (
            PathBuf::from("python3"),
            vec![configured.display().to_string(), query.to_string()],
        );
    }
    (
        configured,
        vec![helper_path().display().to_string(), query.to_string()],
    )
}

fn helper_path() -> PathBuf {
    let packaged = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(HELPER_NAME);
    if packaged.exists() {
        packaged
    } else {
        PathBuf::from(HELPER_NAME)
    }
}

fn disabled_by_env() -> bool {
    std::env::var("DENPIE_DISABLE_DDGS")
        .ok()
        .is_some_and(|value| is_disabled_value(&value))
}

fn is_disabled_value(value: &str) -> bool {
    matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES")
}

fn search_query(query: &str) -> String {
    let query = query.trim();
    let lower = query.to_ascii_lowercase();
    let mut result = query.to_string();
    if !lower.split_whitespace().any(|word| word == "diagram") {
        result.push_str(" diagram");
    }
    // Add file intent only when the caller did not already provide one.  Keep
    // the original topic text intact so specific senses are not generalized.
    if !lower.contains("filetype:")
        && !lower.contains("file type")
        && !lower.contains("image")
        && !lower.contains("photo")
        && !lower.contains("picture")
        && !lower.contains("webp")
        && !lower.contains(".png")
        && !lower.contains(".jpg")
        && !lower.contains(".jpeg")
    {
        result.push_str(" filetype:png OR filetype:jpg");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_adds_conservative_diagram_and_file_intent() {
        assert_eq!(
            search_query("English prepositions in place"),
            "English prepositions in place diagram filetype:png OR filetype:jpg"
        );
    }

    #[test]
    fn query_preserves_existing_intent_and_specificity() {
        assert_eq!(
            search_query("specific diagram filetype:webp"),
            "specific diagram filetype:webp"
        );
        assert_eq!(search_query("image of a cell"), "image of a cell diagram");
        assert_eq!(
            search_query("specific chart webp"),
            "specific chart webp diagram"
        );
    }

    #[test]
    fn argv_keeps_query_as_one_argument() {
        let (_binary, args) = command_spec("a topic diagram");
        assert_eq!(args.last().map(String::as_str), Some("a topic diagram"));
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn output_is_capped_to_http_urls_and_eight_results() {
        let values = (0..12)
            .map(|index| format!("https://example.test/{index}"))
            .chain(["data:image/svg+xml;base64,placeholder".to_string()])
            .collect::<Vec<_>>();
        let bytes = serde_json::to_vec(&values).unwrap();
        let urls = parse_output(&bytes).unwrap();
        assert_eq!(urls.len(), 8);
        assert!(urls.iter().all(|url| url.starts_with("https://")));
    }

    #[test]
    fn malformed_output_is_an_error() {
        assert!(parse_output(b"not json").is_err());
    }

    #[test]
    fn disable_values_are_honored() {
        assert!(is_disabled_value("1"));
        assert!(is_disabled_value(" true "));
        assert!(is_disabled_value("yes"));
        assert!(!is_disabled_value("0"));
        assert!(!is_disabled_value("false"));
    }

    #[test]
    fn extracts_open_graph_images_in_either_attribute_order() {
        assert_eq!(
            extract_page_image(
                r#"<meta content="/images/diagram.png?x=1&amp;y=2" property="og:image">"#,
                "https://lessons.example/card/1",
            )
            .as_deref(),
            Some("https://lessons.example/images/diagram.png?x=1&y=2")
        );
        assert_eq!(
            extract_page_image(
                r#"<meta property='og:image' content='https://cdn.example/chart.jpg'>"#,
                "https://lessons.example/card/1",
            )
            .as_deref(),
            Some("https://cdn.example/chart.jpg")
        );
    }

    #[test]
    fn extracts_image_src_and_rejects_data_placeholders() {
        let html = r#"<meta property="og:image" content="data:image/svg+xml;base64,abc"><link href="real.webp" rel="alternate image_src">"#;
        assert_eq!(
            extract_page_image(html, "https://lessons.example/card/").as_deref(),
            Some("https://lessons.example/card/real.webp")
        );
    }

    #[tokio::test]
    #[ignore = "live DDGS/page smoke; not for CI"]
    async fn live_ddgs_resolves_image_candidates() {
        let pages = super::discover_pages("diagram of in on at prepositions of place")
            .await
            .unwrap();
        assert!(!pages.is_empty());
        let images = super::resolve_image_urls(&pages).await;
        assert!(!images.is_empty());
        assert!(super::super::download_candidates(&images).await.is_some());
    }
}
