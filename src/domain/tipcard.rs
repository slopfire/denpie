use axum::http::StatusCode;
use base64::{Engine, engine::general_purpose::STANDARD};

use super::image;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipcardType {
    Casual,
    Repeatable,
    Manual,
    Custom,
}

// todo huh
impl TipcardType {
    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "casual_tip" => Self::Casual,
            "repeatable_tip" => Self::Repeatable,
            "manual_tip" => Self::Manual,
            "custom_tip" => Self::Custom,
            "srs_tip" => Self::Repeatable,
            _ => Self::Repeatable,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Casual => "casual_tip",
            Self::Repeatable => "repeatable_tip",
            Self::Manual => "manual_tip",
            Self::Custom => "custom_tip",
        }
    }

    pub fn is_queue(self) -> bool {
        matches!(self, Self::Casual | Self::Manual)
    }

    pub fn is_generated(self) -> bool {
        matches!(self, Self::Casual | Self::Repeatable)
    }
}

// todo huuuuh
pub fn normalize_tipcard_type(value: &str, class_name: &str) -> String {
    match value.trim() {
        "casual" | "casual_tip" => "casual_tip".to_string(),
        "repeatable" | "repeatable_tip" | "reword" | "re:word" => "repeatable_tip".to_string(),
        "manual" | "manual_tip" => "manual_tip".to_string(),
        "custom" | "custom_tip" => "custom_tip".to_string(),
        "srs" | "srs_tip" => "repeatable_tip".to_string(),
        "" if matches!(class_name.trim(), "casual" | "casual_tip") => "casual_tip".to_string(),
        "" if matches!(class_name.trim(), "repeatable" | "reword" | "re:word") => {
            "repeatable_tip".to_string()
        }
        "" if matches!(class_name.trim(), "manual" | "manual_tip") => "manual_tip".to_string(),
        "" if matches!(class_name.trim(), "custom" | "custom_tip") => "custom_tip".to_string(),
        _ => TipcardType::Repeatable.as_str().to_string(),
    }
}

pub fn is_queue_tipcard(value: &str) -> bool {
    TipcardType::from_setting(value).is_queue()
}

pub fn validate_image_data(image_data: Vec<String>) -> Result<Vec<String>, (StatusCode, String)> {
    const MAX_IMAGES: usize = 4;
    // Base64 expands payloads by ~4/3; keep enough headroom for four 10 MB decoded images.
    const MAX_TOTAL_CHARS: usize = 56 * 1024 * 1024;
    let mut normalized = Vec::new();
    let mut total_chars = 0usize;

    for image in image_data {
        let image = image.trim().to_string();
        if image.trim().is_empty() {
            continue;
        }
        if normalized.len() >= MAX_IMAGES {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("A tipcard can have at most {MAX_IMAGES} images"),
            ));
        }
        let allowed = image.starts_with("data:image/png;base64,")
            || image.starts_with("data:image/jpeg;base64,")
            || image.starts_with("data:image/jpg;base64,")
            || image.starts_with("data:image/webp;base64,")
            || image.starts_with("data:image/gif;base64,");
        if !allowed {
            return Err((
                StatusCode::BAD_REQUEST,
                "Only PNG, JPEG, WebP, or GIF data URLs are supported".to_string(),
            ));
        }
        let payload = image
            .split_once(',')
            .map(|(_, payload)| payload)
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "Invalid image data URL".to_string(),
                )
            })?;
        let decoded = STANDARD.decode(payload).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Invalid base64 image data".to_string(),
            )
        })?;
        image::validate_decoded_image_size(decoded.len())?;
        total_chars = total_chars.saturating_add(image.len());
        if total_chars > MAX_TOTAL_CHARS {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Attached images are too large (max {} MB total encoded payload)",
                    MAX_TOTAL_CHARS / 1024 / 1024
                ),
            ));
        }
        normalized.push(image);
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::image::MAX_IMAGE_BYTES;

    #[test]
    fn rejects_image_payload_over_ten_megabytes() {
        let oversized = format!(
            "data:image/png;base64,{}",
            STANDARD.encode(vec![0_u8; MAX_IMAGE_BYTES + 1])
        );
        let err = validate_image_data(vec![oversized]).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("10 MB"));
    }
}
