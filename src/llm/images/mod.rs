//! Pluggable image-retrieval subsystem: controls how a generated card gets
//! illustrated. Pure orchestration — the caller supplies pool metadata and the
//! source cards; SQL and byte storage stay in the service/repositories.

pub mod annotate;
mod bing;
mod bing_playwright;
mod ddgs;
mod pool;
pub use annotate::{annotate_image, remove_tag_json, tags_from_json, tags_to_json};

use crate::domain::grounding::ImageStrategy;
use crate::image_compress::prepare_image_bytes;
use crate::image_store::{self, IncomingImage};
use crate::llm::transport::ReasoningConfig;

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
}

/// A retrieved image ready for one storage pass.
#[derive(Debug)]
pub enum RetrievedImage {
    Prepared(crate::image_compress::PreparedImage),
    Pool(i64),
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
    tracing::info!(
        ?strategy,
        topic,
        card_title,
        pool_images,
        "image strategy started"
    );
    let image = match strategy {
        ImageStrategy::None => None,
        ImageStrategy::Pool => pool::retrieve(input).await,
        ImageStrategy::BingHtml => bing::retrieve(input).await,
        ImageStrategy::BingPlaywright => bing_playwright::retrieve(input).await,
        ImageStrategy::DdgsTextOg => ddgs::retrieve(input).await,
    };
    match &image {
        Some(RetrievedImage::Prepared(prepared)) => tracing::info!(
            ?strategy,
            topic,
            card_title,
            image_kind = "prepared",
            image_bytes = prepared.bytes.len(),
            mime_type = prepared.mime_type,
            extension = prepared.extension,
            "image strategy completed"
        ),
        Some(RetrievedImage::Pool(pool_id)) => tracing::info!(
            ?strategy,
            topic,
            card_title,
            image_kind = "pool",
            pool_image_id = pool_id,
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

const MAX_DOWNLOAD_CANDIDATES: usize = 5;

pub(super) async fn download_candidates(urls: &[String]) -> Option<RetrievedImage> {
    let attempt = async {
        let mut attempted = 0;
        for url in urls {
            if candidate_url_rejected(url) {
                tracing::debug!(
                    host = candidate_host(url),
                    "image candidate rejected by policy"
                );
                continue;
            }
            attempted += 1;
            if let Some(image) = download_and_prepare(url).await {
                return Some(image);
            }
            if attempted >= MAX_DOWNLOAD_CANDIDATES {
                break;
            }
        }
        None
    };
    tokio::time::timeout(std::time::Duration::from_secs(30), attempt)
        .await
        .unwrap_or_else(|_| {
            tracing::warn!("image candidate downloads exceeded the total deadline");
            None
        })
}

fn candidate_url_rejected(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return true;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return true;
    }
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return true;
    };
    let path = url.path().to_ascii_lowercase();
    const BLOCKED_HOSTS: &[&str] = &[
        "th.bing.com",
        "mm.bing.net",
        "ftcdn.net",
        "alamy.com",
        "shutterstock.com",
        "istockphoto.com",
        "opengraph.githubassets.com",
    ];
    BLOCKED_HOSTS
        .iter()
        .any(|blocked| host == *blocked || host.ends_with(&format!(".{blocked}")))
        || path.ends_with(".svg")
        || ["logo", "favicon", "avatar", "sprite", "profile_images/"]
            .iter()
            .any(|marker| path.contains(marker))
}

fn candidate_host(value: &str) -> String {
    url::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_default()
}

/// Download and prepare an image once. Host policy lives in
/// `candidate_url_rejected`; the shared downloader then validates DNS and
/// every redirect.
pub(crate) async fn download_and_prepare(url: &str) -> Option<RetrievedImage> {
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

    Some(RetrievedImage::Prepared(prepared))
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
    use super::candidate_url_rejected;

    #[test]
    fn candidate_policy_rejects_thumbnails_stock_and_placeholders() {
        assert!(candidate_url_rejected("https://tse1.mm.bing.net/a.jpg"));
        assert!(candidate_url_rejected("https://as2.ftcdn.net/a.jpg"));
        assert!(candidate_url_rejected("https://example.com/site-logo.png"));
        assert!(candidate_url_rejected("data:image/png;base64,abc"));
        assert!(!candidate_url_rejected(
            "https://english.example/diagram.png"
        ));
    }
}
