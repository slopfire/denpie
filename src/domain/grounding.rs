//! Pure grounding/image strategy enums. No SQL, no IO.

/// How the LLM sources facts when generating a tip card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundingStrategy {
    /// No grounding — single chat completion, current behavior.
    Factual,
    /// Generate, then fact-check against web/document sources.
    CreateAndGround,
    /// Research a topic and generate many cards; keep a hidden pending backlog.
    Agentic,
    /// Retrieve from user-provided documents (PostgreSQL full-text retrieval).
    Rag,
}

impl GroundingStrategy {
    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "factual" => Self::Factual,
            "create_and_ground" => Self::CreateAndGround,
            "agentic" => Self::Agentic,
            "rag" => Self::Rag,
            _ => Self::Factual,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Factual => "factual",
            Self::CreateAndGround => "create_and_ground",
            Self::Agentic => "agentic",
            Self::Rag => "rag",
        }
    }
}

/// External service used for web search and (optionally) remote document extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchProvider {
    Tavily,
    Firecrawl,
}

impl SearchProvider {
    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "firecrawl" => Self::Firecrawl,
            _ => Self::Tavily,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tavily => "tavily",
            Self::Firecrawl => "firecrawl",
        }
    }
}

/// How linked web pages are turned into document text for grounding.
///
/// Scrapling is the main local option (optional CLI). Firecrawl is the cloud
/// path. Direct is the legacy capped HTML strip fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrapeProvider {
    Scrapling,
    Firecrawl,
    Direct,
}

impl ScrapeProvider {
    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "firecrawl" => Self::Firecrawl,
            "direct" => Self::Direct,
            // Default / main option
            _ => Self::Scrapling,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scrapling => "scrapling",
            Self::Firecrawl => "firecrawl",
            Self::Direct => "direct",
        }
    }
}

/// How a card gets illustrated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageStrategy {
    /// No image retrieval — current behavior.
    None,
    /// Select from a user-uploaded image library.
    Pool,
    /// Execute an LLM-specified declarative fetch recipe against an image API.
    Programmatic,
    /// Search the web for a directly hot-linkable image.
    Agentic,
    /// Search the configured external web provider for image results.
    WebSearch,
}

impl ImageStrategy {
    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "none" => Self::None,
            "pool" => Self::Pool,
            "programmatic" => Self::Programmatic,
            "agentic" => Self::Agentic,
            "web_search" => Self::WebSearch,
            _ => Self::None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Pool => "pool",
            Self::Programmatic => "programmatic",
            Self::Agentic => "agentic",
            Self::WebSearch => "web_search",
        }
    }
}

pub const DEFAULT_IMAGE_SOURCES_SETTING: &str = r#"[{"id":"danbooru","name":"Danbooru","kind":"api","enabled":false,"endpoint":"https://danbooru.donmai.us/posts/random.json","query_parameter":"tags","json_path":"file_url","default_tags":"rating:general","api_hosts":"danbooru.donmai.us","search_domains":"","download_hosts":"cdn.donmai.us","instructions":"Use concise Danbooru tags separated by spaces. Prefer tags that describe the card topic without naming UI text."},{"id":"safebooru","name":"Safebooru","kind":"api","enabled":false,"endpoint":"https://safebooru.org/index.php?page=dapi&s=post&q=index&json=1","query_parameter":"tags","json_path":"file_url","default_tags":"rating:safe","api_hosts":"safebooru.org","search_domains":"","download_hosts":"safebooru.org","instructions":"Use concise booru tags separated by spaces."},{"id":"web-search","name":"Web Image Search","kind":"web_search","enabled":false,"endpoint":"","query_parameter":"","json_path":"","default_tags":"","api_hosts":"","search_domains":"","download_hosts":"","instructions":"Prefer official project documentation and repositories. Avoid logos, tracking pixels, placeholders, and decorative images."}]"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSourceKind {
    Api,
    WebSearch,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ImageSource {
    pub id: String,
    pub name: String,
    pub kind: ImageSourceKind,
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub query_parameter: String,
    #[serde(default)]
    pub json_path: String,
    #[serde(default)]
    pub default_tags: String,
    #[serde(default)]
    pub api_hosts: String,
    #[serde(default)]
    pub search_domains: String,
    #[serde(default)]
    pub download_hosts: String,
    #[serde(default)]
    pub instructions: String,
}

impl ImageSource {
    pub fn api_hosts(&self) -> Vec<String> {
        split_hosts(&self.api_hosts)
    }

    pub fn download_hosts(&self) -> Vec<String> {
        split_hosts(&self.download_hosts)
    }

    pub fn search_domains(&self) -> Vec<String> {
        let configured = split_hosts(&self.search_domains);
        if configured.is_empty() {
            self.download_hosts()
        } else {
            configured
        }
    }
}

pub fn image_sources_from_setting(value: &str) -> Vec<ImageSource> {
    serde_json::from_str(value)
        .ok()
        .filter(|sources: &Vec<ImageSource>| !sources.is_empty())
        .unwrap_or_else(default_image_sources)
}

pub fn default_image_sources() -> Vec<ImageSource> {
    serde_json::from_str(DEFAULT_IMAGE_SOURCES_SETTING)
        .expect("built-in image source settings are valid")
}

fn split_hosts(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grounding_round_trips() {
        for strategy in [
            GroundingStrategy::Factual,
            GroundingStrategy::CreateAndGround,
            GroundingStrategy::Agentic,
            GroundingStrategy::Rag,
        ] {
            assert_eq!(GroundingStrategy::from_setting(strategy.as_str()), strategy);
        }
    }

    #[test]
    fn grounding_unknown_defaults_to_factual() {
        assert_eq!(
            GroundingStrategy::from_setting("nonsense"),
            GroundingStrategy::Factual
        );
        assert_eq!(
            GroundingStrategy::from_setting(""),
            GroundingStrategy::Factual
        );
    }

    #[test]
    fn scrape_provider_round_trips_and_defaults() {
        for provider in [
            ScrapeProvider::Scrapling,
            ScrapeProvider::Firecrawl,
            ScrapeProvider::Direct,
        ] {
            assert_eq!(ScrapeProvider::from_setting(provider.as_str()), provider);
        }
        assert_eq!(
            ScrapeProvider::from_setting("unknown"),
            ScrapeProvider::Scrapling
        );
        assert_eq!(ScrapeProvider::from_setting(""), ScrapeProvider::Scrapling);
    }

    #[test]
    fn image_round_trips() {
        for strategy in [
            ImageStrategy::None,
            ImageStrategy::Pool,
            ImageStrategy::Programmatic,
            ImageStrategy::Agentic,
            ImageStrategy::WebSearch,
        ] {
            assert_eq!(ImageStrategy::from_setting(strategy.as_str()), strategy);
        }
    }

    #[test]
    fn image_unknown_defaults_to_none() {
        assert_eq!(ImageStrategy::from_setting("nonsense"), ImageStrategy::None);
        assert_eq!(ImageStrategy::from_setting(""), ImageStrategy::None);
    }

    #[test]
    fn default_image_sources_are_valid_and_disabled() {
        let sources = default_image_sources();
        assert_eq!(sources.len(), 3);
        assert!(sources.iter().all(|source| !source.enabled));
        assert_eq!(sources[0].download_hosts(), ["cdn.donmai.us"]);
    }

    #[test]
    fn invalid_image_sources_fall_back_to_defaults() {
        assert_eq!(
            image_sources_from_setting("not json"),
            default_image_sources()
        );
    }
}
