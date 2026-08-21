//! Checked-in case packs for lab benches. The image pack is the bake-off gold
//! set from `docs/image-fetch-bing-html.md`, the prompt pack is the
//! one-shot/array assembly bake-off set, and the card pack is the
//! repeatable-card UI fixture gallery; all three are data, not live fixtures.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

pub(crate) const DEFAULT_GOLD_CASES_PATH: &str = "lab/cases/images/gold.json";
pub(crate) const DEFAULT_PROMPT_CASES_PATH: &str = "lab/cases/prompts/gold.json";
pub(crate) const DEFAULT_CARD_CASES_PATH: &str = "lab/cases/cards/repeatable-states.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ImageCase {
    pub(crate) id: u64,
    pub(crate) topic_name: String,
    pub(crate) card_title: String,
    pub(crate) card_content: String,
    pub(crate) image_query: String,
    pub(crate) expected: String,
}

pub(crate) fn load_image_cases(path: &str) -> Result<Vec<ImageCase>, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read image case pack `{path}`: {error}"))?;
    let cases: Vec<ImageCase> = serde_json::from_str(&json)
        .map_err(|error| format!("failed to parse image case pack `{path}`: {error}"))?;
    if cases.is_empty() {
        return Err(format!("image case pack `{path}` is empty"));
    }
    ensure_unique_image_ids(path, &cases)?;
    Ok(cases)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PromptCase {
    pub(crate) id: String,
    pub(crate) topic: String,
    #[serde(default)]
    pub(crate) template: Option<String>,
    #[serde(default = "default_prompt_compression")]
    pub(crate) compression: String,
    pub(crate) mode: String,
    #[serde(default = "default_prompt_batch_count")]
    pub(crate) batch_count: usize,
    #[serde(default)]
    pub(crate) existing_titles: Vec<String>,
    #[serde(default)]
    pub(crate) dismissed_titles: Vec<String>,
    #[serde(default)]
    pub(crate) known_items: Vec<String>,
    #[serde(default)]
    pub(crate) difficult_items: Vec<String>,
    #[serde(default)]
    pub(crate) uninterested_items: Vec<String>,
    pub(crate) expected: String,
}

fn default_prompt_compression() -> String {
    "strong".to_string()
}

fn default_prompt_batch_count() -> usize {
    5
}

pub(crate) fn load_prompt_cases(path: &str) -> Result<Vec<PromptCase>, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read prompt case pack `{path}`: {error}"))?;
    let mut cases: Vec<PromptCase> = serde_json::from_str(&json)
        .map_err(|error| format!("failed to parse prompt case pack `{path}`: {error}"))?;
    if cases.is_empty() {
        return Err(format!("prompt case pack `{path}` is empty"));
    }

    for case in &mut cases {
        let compression = case.compression.trim();
        if compression.is_empty() {
            case.compression = "strong".to_string();
        } else {
            case.compression = compression.to_string();
        }
    }

    let mut ids = HashSet::new();
    for case in &cases {
        if !is_safe_case_id(&case.id) {
            return Err(format!(
                "prompt case pack `{path}` case id `{}` is invalid; ids must match [a-z0-9]+(-[a-z0-9]+)*",
                case.id
            ));
        }
        if !ids.insert(&case.id) {
            return Err(format!(
                "prompt case pack `{path}` has duplicate case id `{}`",
                case.id
            ));
        }
        if case.topic.trim().is_empty() {
            return Err(format!(
                "prompt case pack `{path}` case `{}` has an empty topic",
                case.id
            ));
        }
        if !matches!(
            case.compression.as_str(),
            "light" | "balanced" | "strong" | "ultra"
        ) {
            return Err(format!(
                "prompt case pack `{path}` case `{}` has unknown compression `{}` (expected light, balanced, strong, or ultra)",
                case.id, case.compression
            ));
        }
        if !matches!(case.mode.as_str(), "one_shot" | "array") {
            return Err(format!(
                "prompt case pack `{path}` case `{}` has unknown mode `{}` (expected one_shot or array)",
                case.id, case.mode
            ));
        }
        if case.batch_count == 0 {
            return Err(format!(
                "prompt case pack `{path}` case `{}` has batch_count 0 (must be at least 1)",
                case.id
            ));
        }
        if case.expected.trim().is_empty() {
            return Err(format!(
                "prompt case pack `{path}` case `{}` has an empty expected rubric",
                case.id
            ));
        }
    }

    Ok(cases)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CardFixture {
    pub(crate) id: String,
    pub(crate) topic_name: String,
    pub(crate) title: String,
    pub(crate) full_content: String,
    pub(crate) compressed_content: String,
    pub(crate) tipcard_type: String,
    pub(crate) status: String,
    pub(crate) pinned: bool,
    pub(crate) pending_count: u32,
    #[serde(default)]
    pub(crate) review_message: Option<String>,
    pub(crate) notes: String,
}

pub(crate) fn load_card_cases(path: &str) -> Result<Vec<CardFixture>, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read card case pack `{path}`: {error}"))?;
    let cases: Vec<CardFixture> = serde_json::from_str(&json)
        .map_err(|error| format!("failed to parse card case pack `{path}`: {error}"))?;
    if cases.is_empty() {
        return Err(format!("card case pack `{path}` is empty"));
    }

    let mut ids = HashSet::new();
    for case in &cases {
        if !is_safe_case_id(&case.id) {
            return Err(format!(
                "card case pack `{path}` case id `{}` is invalid; ids must match [a-z0-9]+(-[a-z0-9]+)*",
                case.id
            ));
        }
        if !ids.insert(&case.id) {
            return Err(format!(
                "card case pack `{path}` has duplicate case id `{}`",
                case.id
            ));
        }
        if case.topic_name.trim().is_empty() {
            return Err(format!(
                "card case pack `{path}` case `{}` has an empty topic_name",
                case.id
            ));
        }
        if case.title.trim().is_empty() {
            return Err(format!(
                "card case pack `{path}` case `{}` has an empty title",
                case.id
            ));
        }
        if case.full_content.trim().is_empty() {
            return Err(format!(
                "card case pack `{path}` case `{}` has empty full_content",
                case.id
            ));
        }
        if case.tipcard_type != "repeatable_tip" {
            return Err(format!(
                "card case pack `{path}` case `{}` has tipcard_type `{}` (expected repeatable_tip)",
                case.id, case.tipcard_type
            ));
        }
        if !matches!(case.status.as_str(), "active" | "reviewed") {
            return Err(format!(
                "card case pack `{path}` case `{}` has unknown status `{}` (expected active or reviewed)",
                case.id, case.status
            ));
        }
        if case.notes.trim().is_empty() {
            return Err(format!(
                "card case pack `{path}` case `{}` has empty notes",
                case.id
            ));
        }
    }

    Ok(cases)
}

fn ensure_unique_image_ids(path: &str, cases: &[ImageCase]) -> Result<(), String> {
    let mut ids = HashSet::new();
    for case in cases {
        if !ids.insert(case.id) {
            return Err(format!(
                "image case pack `{path}` has duplicate case id `{}`",
                case.id
            ));
        }
    }
    Ok(())
}

/// Prompt and card IDs become artifact filenames, so use this conservative
/// grammar: lowercase ASCII alphanumerics separated by single hyphens.
fn is_safe_case_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !id.contains("--")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_case_pack_loads_with_the_five_bake_off_queries() {
        let cases = load_image_cases(DEFAULT_GOLD_CASES_PATH).expect("gold case pack must load");

        assert_eq!(cases.len(), 5);
        assert_eq!(
            cases.iter().map(|case| case.id).collect::<Vec<_>>(),
            vec![270, 286, 290, 45, 8]
        );

        let queries = cases
            .iter()
            .map(|case| case.image_query.as_str())
            .collect::<Vec<_>>();
        for expected_query in [
            "diagram prepositions of movement from to into toward",
            "diagram of in on at prepositions of place",
            "diagram of adjective order before nouns English grammar",
            "rust clippy pedantic lints screenshot",
            "helix editor modal text editor screenshot",
        ] {
            assert!(
                queries.contains(&expected_query),
                "gold pack must contain `{expected_query}`"
            );
        }

        for case in cases {
            assert!(!case.topic_name.is_empty());
            assert!(!case.card_title.is_empty());
            assert!(!case.card_content.is_empty());
            assert!(!case.expected.is_empty());
        }
    }

    #[test]
    fn card_gold_case_pack_loads_with_the_seven_repeatable_states() {
        let cases = load_card_cases(DEFAULT_CARD_CASES_PATH).expect("card gold pack must load");

        let ids = cases
            .iter()
            .map(|case| case.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "active",
                "pinned",
                "reviewed-hold",
                "await-refill",
                "daily-complete",
                "stacked",
                "llm-error",
            ]
        );
        assert_eq!(cases.len(), 7);

        for case in &cases {
            assert_eq!(case.tipcard_type, "repeatable_tip");
            assert!(!case.topic_name.is_empty());
            assert!(!case.title.is_empty());
            assert!(!case.full_content.is_empty());
            assert!(!case.notes.is_empty());
            assert!(matches!(case.status.as_str(), "active" | "reviewed"));
        }

        let llm_error = cases
            .iter()
            .find(|case| case.id == "llm-error")
            .expect("llm-error fixture must exist");
        assert!(
            llm_error.full_content.starts_with("LLM Error:"),
            "llm-error full_content must start with `LLM Error:`"
        );
    }

    #[test]
    fn prompt_gold_case_pack_loads_with_required_scenarios() {
        let cases =
            load_prompt_cases(DEFAULT_PROMPT_CASES_PATH).expect("prompt gold pack must load");

        assert!(cases.len() >= 5);
        assert!(
            cases.iter().any(|case| case.topic == "English Grammar"),
            "prompt gold pack must contain English Grammar"
        );
        assert!(
            cases
                .iter()
                .any(|case| case.mode == "array" && case.batch_count == 5),
            "prompt gold pack must contain an array case with batch_count 5"
        );
        assert!(
            cases.iter().any(|case| case
                .template
                .as_deref()
                .is_some_and(|template| template.contains("{topic}")
                    && template.contains("{existing_cards}"))),
            "prompt gold pack must contain a custom template with {{topic}} and {{existing_cards}}"
        );

        let grammar = cases
            .iter()
            .find(|case| case.id == "grammar-existing-known")
            .expect("grammar-existing-known case must exist");
        assert_eq!(grammar.template, None);
        assert_eq!(grammar.compression, "strong");
        assert_eq!(grammar.mode, "one_shot");
        assert_eq!(grammar.existing_titles.len(), 2);
        assert_eq!(grammar.known_items, vec!["Prepositions of movement"]);

        let helix = cases
            .iter()
            .find(|case| case.id == "helix-empty")
            .expect("helix-empty case must exist");
        assert_eq!(helix.compression, "strong");
        assert_eq!(helix.mode, "one_shot");
        assert!(helix.existing_titles.is_empty());

        let array = cases
            .iter()
            .find(|case| case.id == "grammar-array-five")
            .expect("grammar-array-five case must exist");
        assert_eq!(array.mode, "array");
        assert_eq!(array.batch_count, 5);
        assert!(!array.existing_titles.is_empty());

        for case in cases {
            assert!(!case.id.trim().is_empty());
            assert!(!case.topic.trim().is_empty());
            assert!(!case.expected.trim().is_empty());
            assert!(matches!(case.mode.as_str(), "one_shot" | "array"));
            assert!(
                matches!(
                    case.compression.as_str(),
                    "light" | "balanced" | "strong" | "ultra"
                ),
                "unknown compression `{}`",
                case.compression
            );
        }
    }

    #[test]
    fn prompt_case_ids_are_filename_safe_and_unique() {
        let path = std::env::temp_dir().join(format!(
            "denpie-lab-prompt-cases-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(
            &path,
            r#"[
              {"id":"../escape","topic":"Topic","mode":"one_shot","expected":"rubric"}
            ]"#,
        )
        .expect("write fixture");
        let path = path.display().to_string();
        assert!(
            load_prompt_cases(&path)
                .unwrap_err()
                .contains("ids must match")
        );
    }

    #[test]
    fn duplicate_case_ids_are_rejected() {
        let path = std::env::temp_dir().join(format!(
            "denpie-lab-card-cases-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(
            &path,
            r#"[
              {"id":"same","topic_name":"Topic","title":"Title","full_content":"Content","compressed_content":"Content","tipcard_type":"repeatable_tip","status":"active","pinned":false,"pending_count":0,"notes":"Notes"},
              {"id":"same","topic_name":"Topic","title":"Title","full_content":"Content","compressed_content":"Content","tipcard_type":"repeatable_tip","status":"active","pinned":false,"pending_count":0,"notes":"Notes"}
            ]"#,
        )
        .expect("write fixture");
        let path = path.display().to_string();
        assert!(
            load_card_cases(&path)
                .unwrap_err()
                .contains("duplicate case id")
        );
    }
}
