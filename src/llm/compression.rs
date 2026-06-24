#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionLevel {
    Light,
    Balanced,
    Strong,
    Ultra,
}

impl CompressionLevel {
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "strong" => Self::Strong,
            "ultra" => Self::Ultra,
            _ => Self::Balanced,
        }
    }

    pub fn as_setting(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Balanced => "balanced",
            Self::Strong => "strong",
            Self::Ultra => "ultra",
        }
    }

    pub fn reasoning_effort(self) -> &'static str {
        match self {
            Self::Light => "minimal",
            Self::Balanced => "low",
            Self::Strong => "medium",
            Self::Ultra => "high",
        }
    }

    pub(crate) fn prompt_rules(self) -> &'static str {
        match self {
            Self::Light => {
                "Preset: Light compression.\n\
                 - Preserve nearly all useful detail, examples, and caveats.\n\
                 - Target 110-150 words, or about 650-900 characters.\n\
                 - Prefer trimming connective prose over removing steps.\n\
                 - Fenced code blocks are preserved separately; compress prose only."
            }
            Self::Balanced => {
                "Preset: Balanced compression.\n\
                 - Preserve the most useful actionable details; never reduce it to a vague teaser.\n\
                 - Target 70-110 words, or about 420-650 characters.\n\
                 - Keep markdown if it improves scanning on mobile.\n\
                 - Fenced code blocks are preserved separately; compress prose only."
            }
            Self::Strong => {
                "Preset: Strong compression.\n\
                 - Keep only the core idea, the highest-value example or command, and one caveat when important.\n\
                 - Target 40-70 words, or about 250-420 characters.\n\
                 - Use tight bullets or compact sentences.\n\
                 - Fenced code blocks are preserved separately; compress prose only."
            }
            Self::Ultra => {
                "Preset: Ultra compression.\n\
                 - Return a reminder-sized card with the action, trigger, and critical caveat only.\n\
                 - Target 18-35 words, or about 120-240 characters.\n\
                 - Avoid setup, explanation, and nice-to-have context.\n\
                 - Fenced code blocks are preserved separately; compress prose only."
            }
        }
    }

    pub(crate) fn oneshot_target(self) -> &'static str {
        match self {
            Self::Light => "110-150 words, or about 650-900 characters",
            Self::Balanced => "70-110 words, or about 420-650 characters",
            Self::Strong => "40-70 words, or about 250-420 characters",
            Self::Ultra => "18-35 words, or about 120-240 characters",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CompressionLevel;

    #[test]
    fn compression_level_normalizes_settings() {
        assert_eq!(
            CompressionLevel::from_setting(" ULTRA ").as_setting(),
            "ultra"
        );
        assert_eq!(
            CompressionLevel::from_setting("unknown").as_setting(),
            "balanced"
        );
    }

    #[test]
    fn compression_level_selects_reasoning_effort() {
        assert_eq!(CompressionLevel::Light.reasoning_effort(), "minimal");
        assert_eq!(CompressionLevel::Balanced.reasoning_effort(), "low");
        assert_eq!(CompressionLevel::Strong.reasoning_effort(), "medium");
        assert_eq!(CompressionLevel::Ultra.reasoning_effort(), "high");
    }
}
