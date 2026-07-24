//! Image-source configuration for grounding settings.
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ImageSourceKind {
    Api,
    WebSearch,
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq)]
pub(crate) struct ImageSourceSettings {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: ImageSourceKind,
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) endpoint: String,
    #[serde(default)]
    pub(crate) query_parameter: String,
    #[serde(default)]
    pub(crate) json_path: String,
    #[serde(default)]
    pub(crate) default_tags: String,
    #[serde(default)]
    pub(crate) api_hosts: String,
    #[serde(default)]
    pub(crate) download_hosts: String,
    #[serde(default)]
    pub(crate) instructions: String,
}

pub(crate) const DEFAULT_IMAGE_SOURCES: &str = r#"[{"id":"danbooru","name":"Danbooru","kind":"api","enabled":false,"endpoint":"https://danbooru.donmai.us/posts/random.json","query_parameter":"tags","json_path":"file_url","default_tags":"rating:general","api_hosts":"danbooru.donmai.us","download_hosts":"cdn.donmai.us","instructions":"Use concise Danbooru tags separated by spaces. Prefer tags that describe the card topic without naming UI text."},{"id":"safebooru","name":"Safebooru","kind":"api","enabled":false,"endpoint":"https://safebooru.org/index.php?page=dapi&s=post&q=index&json=1","query_parameter":"tags","json_path":"file_url","default_tags":"rating:safe","api_hosts":"safebooru.org","download_hosts":"safebooru.org","instructions":"Use concise booru tags separated by spaces."},{"id":"web-search","name":"Web Image Search","kind":"web_search","enabled":false,"endpoint":"","query_parameter":"","json_path":"","default_tags":"","api_hosts":"","download_hosts":"","instructions":"Prefer official project documentation and repositories. Return a direct image asset, never a webpage, logo, tracking pixel, or placeholder."}]"#;

pub(crate) fn parse_image_sources(value: &str) -> Vec<ImageSourceSettings> {
    serde_json::from_str(value)
        .ok()
        .filter(|sources: &Vec<ImageSourceSettings>| !sources.is_empty())
        .unwrap_or_else(|| {
            serde_json::from_str(DEFAULT_IMAGE_SOURCES)
                .expect("built-in frontend image sources are valid")
        })
}
