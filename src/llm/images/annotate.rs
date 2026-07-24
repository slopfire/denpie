//! Vision-based image annotation: ask a vision model to produce a short name,
//! a one-sentence description, and 1-5 tags for a pool image. Used when a user
//! uploads an image so the pool is searchable and sortable without manual entry.
use serde::Deserialize;

use crate::llm::transport::create_vision_completion;

/// The annotation the vision model returns for an image.
#[derive(Clone, Debug, Default)]
pub struct ImageAnnotation {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

/// Ask the vision model to annotate `image_data_url` (a `data:<mime>;base64,…`
/// URL). Returns `None` on any failure so the caller can fall back to the
/// original filename.
pub async fn annotate_image(
    model: &str,
    image_data_url: &str,
    api_key: &str,
    api_base: &str,
) -> Option<ImageAnnotation> {
    if model.is_empty() || api_key.is_empty() {
        return None;
    }

    let prompt = "\
        Look at this image and respond with JSON only, no markdown fences. \
        The JSON must have exactly these keys:\n\
        - \"name\": a short descriptive filename (2-5 words, no extension, lowercase with hyphens)\n\
        - \"description\": one concise sentence describing the image\n\
        - \"tags\": an array of 1-5 single-word lowercase tags that capture the subject, style, or mood\n\n\
        Example: {\"name\":\"red-sunset-over-mountains\",\"description\":\"A vibrant red sunset behind a mountain range.\",\"tags\":[\"sunset\",\"mountains\",\"nature\",\"landscape\"]}";

    let response =
        create_vision_completion(model, prompt, image_data_url, api_key, api_base, Some(256)).await;

    if response.content.is_empty() || response.content.starts_with("LLM Error") {
        tracing::warn!(
            content = %response.content,
            "Vision annotation failed, falling back"
        );
        return None;
    }

    let annotation = parse_annotation(&response.content);
    if annotation.is_none() {
        tracing::warn!(
            content = %response.content,
            "Vision annotation returned an invalid payload, falling back"
        );
    }
    annotation
}

fn parse_annotation(content: &str) -> Option<ImageAnnotation> {
    let trimmed = content.trim();
    let json_text = trimmed
        .strip_prefix("```json")
        .and_then(|v| v.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);

    #[derive(Deserialize)]
    struct RawAnnotation {
        name: Option<String>,
        description: Option<String>,
        tags: Option<Vec<String>>,
    }

    let parsed: RawAnnotation = match serde_json::from_str(json_text) {
        Ok(p) => p,
        Err(_) => {
            // Try to extract the first {...} block if there's surrounding text
            let start = json_text.find('{')?;
            let end = json_text.rfind('}')?;
            serde_json::from_str(&json_text[start..=end]).ok()?
        }
    };

    let name = parsed.name?.trim().to_string();
    if name.is_empty() {
        return None;
    }

    Some(ImageAnnotation {
        name,
        description: parsed.description.unwrap_or_default().trim().to_string(),
        tags: parsed
            .tags
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .take(5)
            .collect(),
    })
}

/// Serialize a list of tags into the JSON array string stored in the DB.
pub fn tags_to_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
}

/// Parse the JSON array string from the DB back into a Vec<String>.
pub fn tags_from_json(raw: &str) -> Vec<String> {
    if raw.is_empty() {
        return Vec::new();
    }
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

/// Remove a single tag from the JSON array string, returning the new JSON.
pub fn remove_tag_json(raw: &str, tag: &str) -> String {
    let mut tags = tags_from_json(raw);
    tags.retain(|t| t != tag);
    tags_to_json(&tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_annotation() {
        let ann = parse_annotation(
            r#"{"name":"red-sunset","description":"A red sunset.","tags":["sunset","nature"]}"#,
        )
        .unwrap();
        assert_eq!(ann.name, "red-sunset");
        assert_eq!(ann.description, "A red sunset.");
        assert_eq!(ann.tags, vec!["sunset", "nature"]);
    }

    #[test]
    fn parse_fenced_annotation() {
        let ann = parse_annotation(
            "```json\n{\"name\":\"cat\",\"description\":\"A cat.\",\"tags\":[\"animal\"]}\n```",
        )
        .unwrap();
        assert_eq!(ann.name, "cat");
        assert_eq!(ann.tags, vec!["animal"]);
    }

    #[test]
    fn parse_with_surrounding_text() {
        let ann = parse_annotation(
            r#"Here is the result: {"name":"dog","description":"A dog.","tags":["animal"]} done"#,
        )
        .unwrap();
        assert_eq!(ann.name, "dog");
    }

    #[test]
    fn parse_missing_name_returns_none() {
        assert!(parse_annotation(r#"{"description":"no name","tags":[]}"#).is_none());
    }

    #[test]
    fn tags_roundtrip() {
        let tags = vec!["sunset".to_string(), "nature".to_string()];
        let json = tags_to_json(&tags);
        assert_eq!(tags_from_json(&json), tags);
    }

    #[test]
    fn remove_tag_works() {
        let json = tags_to_json(&["a".to_string(), "b".to_string(), "c".to_string()]);
        let result = remove_tag_json(&json, "b");
        assert_eq!(tags_from_json(&result), vec!["a", "c"]);
    }

    #[test]
    fn remove_nonexistent_tag_noop() {
        let json = tags_to_json(&["a".to_string(), "b".to_string()]);
        let result = remove_tag_json(&json, "z");
        assert_eq!(tags_from_json(&result), vec!["a", "b"]);
    }

    #[test]
    fn tags_from_empty_returns_vec() {
        assert!(tags_from_json("").is_empty());
        assert!(tags_from_json("invalid").is_empty());
    }
}
