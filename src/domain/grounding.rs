//! Pure grounding/image strategy enums. No SQL, no IO.

use serde::{Deserialize, Serialize};

macro_rules! string_enum {
    ($name:ident, $default:ident, {$($value:literal => $variant:ident),+ $(,)?}) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn from_setting(value: &str) -> Self {
                match value.trim() { $($value => Self::$variant,)+ _ => Self::$default }
            }

            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value,)+ }
            }
        }

        impl Default for $name {
            fn default() -> Self { Self::$default }
        }
    };
}

string_enum!(GroundingSourceMode, Model, {
    "model" => Model,
    "web" => Web,
    "documents" => Documents,
    "web_and_documents" => WebAndDocuments,
});

impl GroundingSourceMode {
    pub fn uses_web(self) -> bool {
        matches!(self, Self::Web | Self::WebAndDocuments)
    }

    pub fn uses_documents(self) -> bool {
        matches!(self, Self::Documents | Self::WebAndDocuments)
    }
}

string_enum!(VerificationMode, Standard, {
    "standard" => Standard,
    "thorough" => Thorough,
});

string_enum!(GroundingBehavior, Standard, {
    "standard" => Standard,
    "fact_check" => FactCheck,
    "agentic" => Agentic,
});

impl GroundingBehavior {
    pub fn from_legacy_verification(value: VerificationMode) -> Self {
        match value {
            VerificationMode::Standard => Self::Standard,
            VerificationMode::Thorough => Self::FactCheck,
        }
    }

    pub fn legacy_verification(self) -> VerificationMode {
        match self {
            Self::Standard | Self::Agentic => VerificationMode::Standard,
            Self::FactCheck => VerificationMode::Thorough,
        }
    }
}

string_enum!(SearchDepth, Balanced, {
    "quick" => Quick,
    "balanced" => Balanced,
    "deep" => Deep,
});

impl SearchDepth {
    pub fn result_limit(self) -> usize {
        match self {
            Self::Quick => 3,
            Self::Balanced => 6,
            Self::Deep => 12,
        }
    }
}

string_enum!(Freshness, Any, {
    "any" => Any,
    "week" => Week,
    "month" => Month,
    "year" => Year,
});

string_enum!(FailurePolicy, Stop, {
    "stop" => Stop,
    "mark_unverified" => MarkUnverified,
});

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GroundingSearchProfile {
    pub depth: SearchDepth,
    pub freshness: Freshness,
    pub guidance: String,
    pub include_domains: Vec<String>,
    pub failure_policy: FailurePolicy,
}

impl Default for GroundingSearchProfile {
    fn default() -> Self {
        Self {
            depth: SearchDepth::Balanced,
            freshness: Freshness::Any,
            guidance: String::new(),
            include_domains: Vec::new(),
            failure_policy: FailurePolicy::Stop,
        }
    }
}

impl GroundingSearchProfile {
    pub fn normalized(mut self) -> Self {
        self.guidance = self.guidance.trim().to_string();
        self.include_domains = self
            .include_domains
            .into_iter()
            .map(|domain| domain.trim().to_ascii_lowercase())
            .filter(|domain| !domain.is_empty())
            .collect();
        self.include_domains.sort();
        self.include_domains.dedup();
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroundingPolicy {
    pub source_mode: GroundingSourceMode,
    pub behavior: GroundingBehavior,
    pub search_profile: GroundingSearchProfile,
}

impl GroundingPolicy {
    pub fn normalized(mut self) -> Self {
        if self.source_mode == GroundingSourceMode::Model {
            self.behavior = GroundingBehavior::Standard;
        }
        self.search_profile = self.search_profile.normalized();
        self
    }

    pub fn from_legacy(strategy: GroundingStrategy) -> Self {
        let (source_mode, behavior) = match strategy {
            GroundingStrategy::Factual => (GroundingSourceMode::Model, GroundingBehavior::Standard),
            GroundingStrategy::Agentic => (GroundingSourceMode::Web, GroundingBehavior::Agentic),
            GroundingStrategy::CreateAndGround => {
                (GroundingSourceMode::Web, GroundingBehavior::FactCheck)
            }
            GroundingStrategy::Rag => (GroundingSourceMode::Documents, GroundingBehavior::Standard),
        };
        Self {
            source_mode,
            behavior,
            search_profile: GroundingSearchProfile::default(),
        }
    }

    pub fn legacy_strategy(&self) -> GroundingStrategy {
        match (self.source_mode, self.behavior) {
            (GroundingSourceMode::Model, _) => GroundingStrategy::Factual,
            (GroundingSourceMode::Documents, GroundingBehavior::Standard) => GroundingStrategy::Rag,
            (
                GroundingSourceMode::Web
                | GroundingSourceMode::Documents
                | GroundingSourceMode::WebAndDocuments,
                GroundingBehavior::FactCheck,
            ) => GroundingStrategy::CreateAndGround,
            (
                GroundingSourceMode::Web | GroundingSourceMode::WebAndDocuments,
                GroundingBehavior::Standard | GroundingBehavior::Agentic,
            ) => GroundingStrategy::Agentic,
            (GroundingSourceMode::Documents, GroundingBehavior::Agentic) => GroundingStrategy::Rag,
        }
    }

    pub fn legacy_verification_mode(&self) -> VerificationMode {
        self.behavior.legacy_verification()
    }
}

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
    Native,
    Tavily,
    Firecrawl,
}

impl SearchProvider {
    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "native" => Self::Native,
            "firecrawl" => Self::Firecrawl,
            _ => Self::Tavily,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
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
    fn legacy_strategies_map_to_typed_policies() {
        let cases = [
            (
                GroundingStrategy::Factual,
                GroundingSourceMode::Model,
                GroundingBehavior::Standard,
            ),
            (
                GroundingStrategy::Agentic,
                GroundingSourceMode::Web,
                GroundingBehavior::Agentic,
            ),
            (
                GroundingStrategy::CreateAndGround,
                GroundingSourceMode::Web,
                GroundingBehavior::FactCheck,
            ),
            (
                GroundingStrategy::Rag,
                GroundingSourceMode::Documents,
                GroundingBehavior::Standard,
            ),
        ];
        for (legacy, source_mode, behavior) in cases {
            let policy = GroundingPolicy::from_legacy(legacy);
            assert_eq!(policy.source_mode, source_mode);
            assert_eq!(policy.behavior, behavior);
            assert_eq!(policy.search_profile.failure_policy, FailurePolicy::Stop);
        }
    }

    #[test]
    fn model_agentic_normalizes_to_standard() {
        let policy = GroundingPolicy {
            source_mode: GroundingSourceMode::Model,
            behavior: GroundingBehavior::Agentic,
            search_profile: GroundingSearchProfile::default(),
        }
        .normalized();
        assert_eq!(policy.behavior, GroundingBehavior::Standard);
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
