use axum::http::StatusCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipcardType {
    Srs,
    Casual,
    Repeatable,
    Manual,
    Custom,
}

impl TipcardType {
    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "casual_tip" => Self::Casual,
            "repeatable_tip" => Self::Repeatable,
            "manual_tip" => Self::Manual,
            "custom_tip" => Self::Custom,
            _ => Self::Srs,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Srs => "srs_tip",
            Self::Casual => "casual_tip",
            Self::Repeatable => "repeatable_tip",
            Self::Manual => "manual_tip",
            Self::Custom => "custom_tip",
        }
    }

    pub fn is_queue(self) -> bool {
        matches!(self, Self::Casual | Self::Repeatable | Self::Manual)
    }

    pub fn is_generated(self) -> bool {
        matches!(self, Self::Srs | Self::Casual | Self::Repeatable)
    }
}

pub fn normalize_tipcard_type(value: &str, class_name: &str) -> String {
    match value.trim() {
        "casual" | "casual_tip" => "casual_tip".to_string(),
        "repeatable" | "repeatable_tip" | "reword" | "re:word" => "repeatable_tip".to_string(),
        "manual" | "manual_tip" => "manual_tip".to_string(),
        "custom" | "custom_tip" => "custom_tip".to_string(),
        "srs" | "srs_tip" => "srs_tip".to_string(),
        "" if matches!(class_name.trim(), "casual" | "casual_tip") => "casual_tip".to_string(),
        "" if matches!(class_name.trim(), "repeatable" | "reword" | "re:word") => {
            "repeatable_tip".to_string()
        }
        "" if matches!(class_name.trim(), "manual" | "manual_tip") => "manual_tip".to_string(),
        "" if matches!(class_name.trim(), "custom" | "custom_tip") => "custom_tip".to_string(),
        _ => TipcardType::Srs.as_str().to_string(),
    }
}

pub fn is_queue_tipcard(value: &str) -> bool {
    TipcardType::from_setting(value).is_queue()
}

pub fn validate_image_data(image_data: Vec<String>) -> Result<Vec<String>, (StatusCode, String)> {
    const MAX_IMAGES: usize = 4;
    const MAX_TOTAL_CHARS: usize = 12 * 1024 * 1024;
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
        total_chars = total_chars.saturating_add(image.len());
        if total_chars > MAX_TOTAL_CHARS {
            return Err((
                StatusCode::BAD_REQUEST,
                "Attached images are too large".to_string(),
            ));
        }
        normalized.push(image);
    }

    Ok(normalized)
}
