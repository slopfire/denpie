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
    /// Fetch Bing Images search HTML and download a source image from its metadata.
    BingHtml,
    /// Render Bing Images with an optional local Playwright sidecar.
    BingPlaywright,
    /// Search the web with DDGS and resolve page Open Graph images.
    DdgsTextOg,
}

impl ImageStrategy {
    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "none" => Self::None,
            "pool" => Self::Pool,
            "bing_html" => Self::BingHtml,
            "bing_playwright" => Self::BingPlaywright,
            "ddgs_text_og" => Self::DdgsTextOg,
            // The removed remote strategies did not work without external API
            // configuration. Keep stored user/topic values useful by migrating
            // them at the parsing boundary to the keyless default.
            "programmatic" | "agentic" | "web_search" => Self::BingHtml,
            _ => Self::None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Pool => "pool",
            Self::BingHtml => "bing_html",
            Self::BingPlaywright => "bing_playwright",
            Self::DdgsTextOg => "ddgs_text_og",
        }
    }
}

/// Unused leftover wire field. Strategies no longer read per-source JSON;
/// keep a valid empty array so older API clients still round-trip.
pub const DEFAULT_IMAGE_SOURCES_SETTING: &str = "[]";

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
            ImageStrategy::BingHtml,
            ImageStrategy::BingPlaywright,
            ImageStrategy::DdgsTextOg,
        ] {
            assert_eq!(ImageStrategy::from_setting(strategy.as_str()), strategy);
        }
    }

    #[test]
    fn removed_remote_strategies_migrate_to_bing_html() {
        for legacy in ["programmatic", "agentic", "web_search"] {
            assert_eq!(ImageStrategy::from_setting(legacy), ImageStrategy::BingHtml);
        }
    }

    #[test]
    fn image_unknown_defaults_to_none() {
        assert_eq!(ImageStrategy::from_setting("nonsense"), ImageStrategy::None);
        assert_eq!(ImageStrategy::from_setting(""), ImageStrategy::None);
    }

    #[test]
    fn legacy_image_source_default_remains_valid_json_for_wire_compatibility() {
        let sources: serde_json::Value =
            serde_json::from_str(DEFAULT_IMAGE_SOURCES_SETTING).unwrap();
        assert_eq!(sources.as_array().map(Vec::len), Some(0));
    }
}
