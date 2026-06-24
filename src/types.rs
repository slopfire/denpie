use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

pub type ApiResult<T> = Result<T, (StatusCode, String)>;

#[derive(Clone, Deserialize)]
pub struct TipsJsonRequest {
    pub count: Option<u32>,
    pub topics: String,
    pub tipcard_type: Option<String>,
    pub exclude_card_ids: Option<Vec<i64>>,
    pub manual_content: Option<String>,
    pub manual_compressed_content: Option<String>,
    pub manual_image_data: Option<Vec<String>>,
}

#[derive(Clone, Serialize)]
pub struct TipCardJson {
    pub id: i64,
    pub topic: String,
    pub full_content: String,
    pub compressed_content: String,
    pub image_data: Vec<String>,
    pub tipcard_type: String,
    pub pinned: bool,
}

#[derive(Deserialize)]
pub struct ReviewJsonRequest {
    pub card_id: i64,
    pub grade: Option<u8>,
    pub action: Option<String>,
}

#[derive(Deserialize)]
pub struct ForceDailyRefreshRequest {
    pub topics: String,
    pub tipcard_type: Option<String>,
}

#[derive(Serialize)]
pub struct ForceDailyRefreshResponse {
    pub refreshed_cards: u64,
}
