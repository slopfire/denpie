use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{db::repositories::topics, domain};

#[derive(Clone)]
pub(crate) struct TopicInfo {
    pub(crate) id: i64,
    pub(crate) tipcard_type: String,
    pub(crate) prompt_template: Option<String>,
    pub(crate) daily_card_count: Option<i64>,
    pub(crate) daily_time_zone: Option<String>,
    pub(crate) daily_update_time: Option<String>,
    pub(crate) compression_level: Option<String>,
}

impl domain::scheduling::DailyWindowTopic for TopicInfo {
    fn daily_card_count(&self) -> Option<i64> {
        self.daily_card_count
    }

    fn daily_time_zone(&self) -> Option<&str> {
        self.daily_time_zone.as_deref()
    }

    fn daily_update_time(&self) -> Option<&str> {
        self.daily_update_time.as_deref()
    }
}

impl From<topics::TopicRecord> for TopicInfo {
    fn from(record: topics::TopicRecord) -> Self {
        Self {
            id: record.id,
            tipcard_type: record.tipcard_type,
            prompt_template: record.prompt_template,
            daily_card_count: record.daily_card_count,
            daily_time_zone: record.daily_time_zone,
            daily_update_time: record.daily_update_time,
            compression_level: record.compression_level,
        }
    }
}

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

pub(crate) type ApiResult<T> = Result<T, (StatusCode, String)>;
