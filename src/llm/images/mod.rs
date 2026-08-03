//! Pluggable image-retrieval subsystem: controls how a generated card gets
//! illustrated. Pure orchestration — the caller supplies pool metadata and the
//! source cards; SQL and byte storage stay in the service/repositories.

mod agentic;
pub mod annotate;
mod pool;
mod programmatic;
mod web_search;
pub use annotate::{annotate_image, remove_tag_json, tags_from_json, tags_to_json};

use crate::domain::grounding::{ImageSource, ImageStrategy};
use crate::image_compress::prepare_image_bytes;
use crate::image_store::{self, IncomingImage};
use crate::llm::transport::ReasoningConfig;
use base64::{Engine, engine::general_purpose::STANDARD};

/// Metadata for a pool image, shown to the model so it can pick the best match.
#[derive(Clone, Debug)]
pub struct PoolImageMeta {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
}

/// Inputs to an image-retrieval strategy. Borrows everything.
pub struct ImageInput<'a> {
    pub topic_name: &'a str,
    pub card_title: &'a str,
    pub card_content: &'a str,
    pub image_query: &'a str,
    pub model: &'a str,
    pub api_key: &'a str,
    pub api_base: &'a str,
    pub reasoning: &'a ReasoningConfig,
    pub pool: &'a [PoolImageMeta],
    pub sources: &'a [ImageSource],
    pub search_api_key: &'a str,
    pub search_base_url: &'a str,
    pub search_provider: &'a str,
}

/// A retrieved image, ready for `image_store::replace_card_images`.
#[derive(Clone, Debug)]
pub struct RetrievedImage {
    /// base64 data-URL (`data:<mime>;base64,<...>`).
    pub data_url: String,
    /// Set only by the `pool` strategy: the chosen pool image id, so the service
    /// can load its bytes from disk instead of re-fetching.
    pub pool_id: Option<i64>,
}

/// Dispatch on the configured image strategy. The image extension point: a new
/// strategy = new enum arm + new arm here + new module. Returns `None` on any
/// failure so the card is served without an image rather than erroring.
pub async fn retrieve_image(
    strategy: ImageStrategy,
    input: ImageInput<'_>,
) -> Option<RetrievedImage> {
    let topic = input.topic_name;
    let card_title = input.card_title;
    let pool_images = input.pool.len();
    let configured_sources = input.sources.iter().filter(|source| source.enabled).count();
    tracing::info!(
        ?strategy,
        topic,
        card_title,
        pool_images,
        configured_sources,
        "image strategy started"
    );
    let image = match strategy {
        ImageStrategy::None => None,
        ImageStrategy::Pool => pool::retrieve(input).await,
        ImageStrategy::Programmatic => programmatic::retrieve(input).await,
        ImageStrategy::Agentic => agentic::retrieve(input).await,
        ImageStrategy::WebSearch => web_search::retrieve(input).await,
    };
    match &image {
        Some(image) => tracing::info!(
            ?strategy,
            topic,
            card_title,
            pool_id = image.pool_id,
            data_url_len = image.data_url.len(),
            "image strategy completed"
        ),
        None => tracing::info!(
            ?strategy,
            topic,
            card_title,
            "image strategy completed without an image"
        ),
    }
    image
}

/// Validate that `url`'s host is in the allowlist. Returns the parsed host on
/// success.
pub(crate) fn host_allowed(url: &str, allowed_hosts: &[String]) -> bool {
    match url::Url::parse(url) {
        Ok(parsed) => match parsed.host_str() {
            Some(host) => allowed_hosts
                .iter()
                .any(|allowed| allowed.trim().eq_ignore_ascii_case(host)),
            None => false,
        },
        Err(_) => false,
    }
}

/// Download an image from an allowlisted URL, recompress it, and base64-encode it
/// to a data-URL. The shared hardened downloader validates DNS and every redirect.
pub(crate) async fn download_and_prepare(
    url: &str,
    allowed_hosts: &[String],
) -> Option<RetrievedImage> {
    if !host_allowed(url, allowed_hosts) {
        tracing::warn!(url, "image url host not in allowlist; skipping");
        return None;
    }

    tracing::info!(url, "downloading strategy-selected image");
    let incoming = match image_store::download_remote_image(url).await {
        Ok(incoming) => incoming,
        Err(err) => {
            tracing::warn!(error = ?err, "image fetch failed safety validation");
            return None;
        }
    };
    let IncomingImage::Bytes { bytes, mime_type } = incoming else {
        return None;
    };
    let (_, extension) = mime_and_extension(&mime_type)?;
    let prepared = match prepare_image_bytes(bytes, &mime_type, extension) {
        Ok(prepared) => prepared,
        Err(err) => {
            tracing::warn!(%err, "image recompression failed");
            return None;
        }
    };

    tracing::info!(
        url,
        prepared_bytes = prepared.bytes.len(),
        mime_type = prepared.mime_type,
        "strategy-selected image downloaded and prepared"
    );

    let encoded = STANDARD.encode(&prepared.bytes);
    Some(RetrievedImage {
        data_url: format!("data:{};base64,{}", prepared.mime_type, encoded),
        pool_id: None,
    })
}

fn mime_and_extension(content_type: &str) -> Option<(&'static str, &'static str)> {
    // Strip any `; charset=...` suffix.
    let base = content_type.split(';').next().unwrap_or("").trim();
    match base {
        "image/png" => Some(("image/png", "png")),
        "image/jpeg" | "image/jpg" => Some(("image/jpeg", "jpg")),
        "image/webp" => Some(("image/webp", "webp")),
        "image/gif" => Some(("image/gif", "gif")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::host_allowed;

    #[test]
    fn host_allowed_matches_allowlist() {
        let allowed = vec![
            "danbooru.donmau.us".to_string(),
            "safebooru.org".to_string(),
        ];
        assert!(host_allowed(
            "https://danbooru.donmau.us/posts/random.json",
            &allowed
        ));
        assert!(host_allowed("https://safebooru.org/img.png", &allowed));
    }

    #[test]
    fn host_not_in_allowlist_rejected() {
        let allowed = vec!["danbooru.donmau.us".to_string()];
        assert!(!host_allowed("https://evil.example.com/x.png", &allowed));
        assert!(!host_allowed("not a url", &allowed));
    }

    #[test]
    fn host_match_is_case_insensitive() {
        let allowed = vec!["Danbooru.Donmau.us".to_string()];
        assert!(host_allowed("https://danbooru.donmau.us/x.png", &allowed));
    }
}
