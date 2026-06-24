use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct SettingsRes {
    pub server_version: String,
    pub build_sha: String,
    pub model: String,
    pub compress_model: String,
    pub template: String,
    pub api_key: String,
    pub base_url: String,
    pub compress_base_url: String,
    pub reasoning_effort: String,
    pub compress_reasoning_effort: String,
    pub compression_level: String,
    pub color_scheme: String,
    pub transparency: String,
    pub blur_intensity: String,
    pub autoupdate_enabled: bool,
    pub autoupdate_repo: String,
    pub autoupdate_branch: String,
    pub autoupdate_check_interval_secs: u64,
    pub autoupdate_command: String,
    pub autoupdate_last_seen_sha: String,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub max_active_cards: u64,
}

#[derive(Deserialize)]
pub struct UpdateSettingsReq {
    pub model: Option<String>,
    pub compress_model: Option<String>,
    pub template: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub compress_base_url: Option<String>,
    pub reasoning_effort: Option<String>,
    pub compress_reasoning_effort: Option<String>,
    pub compression_level: Option<String>,
    pub color_scheme: Option<String>,
    pub transparency: Option<String>,
    pub blur_intensity: Option<String>,
    pub ui_blur: Option<String>,
    pub autoupdate_enabled: Option<bool>,
    pub autoupdate_repo: Option<String>,
    pub autoupdate_branch: Option<String>,
    pub autoupdate_check_interval_secs: Option<u64>,
    pub autoupdate_command: Option<String>,
    pub daily_time_zone: Option<String>,
    pub daily_update_time: Option<String>,
    pub max_active_cards: Option<u64>,
}

#[derive(Serialize)]
pub struct TriggerAutoupdateRes {
    pub message: String,
    pub restarting: bool,
    pub updating: bool,
    pub target_sha: Option<String>,
    pub build_sha: String,
}

#[derive(Deserialize)]
pub struct CreateKeyReq {
    pub client_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub client_name: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct DeleteKeyReq {
    pub id: i64,
}

#[derive(Deserialize)]
pub struct DeleteTipcardReq {
    pub id: i64,
}

#[derive(Deserialize)]
pub struct PinTipcardReq {
    pub id: i64,
    pub pinned: Option<bool>,
    pub image_data: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct TopicInfo {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
    pub icon_id: String,
    pub topic_color: String,
    pub prompt_template: String,
    pub daily_card_count: u32,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub compression_level: String,
}

#[derive(Serialize)]
pub struct TokenSpend {
    pub daily: i64,
    pub monthly: i64,
    pub total: i64,
}

#[derive(Serialize)]
pub struct AppSummary {
    pub topics: i64,
    pub total_cards: i64,
    pub due_cards: i64,
    pub active_cards: i64,
}

#[derive(Serialize)]
pub struct AppTopicInfo {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
    pub icon_id: String,
    pub topic_color: String,
    pub prompt_template: String,
    pub total_cards: i64,
    pub due_cards: i64,
    pub completed_cards: i64,
    pub daily_card_count: u32,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub compression_level: String,
}

#[derive(Deserialize)]
pub struct UpdateTopicReq {
    pub id: i64,
    pub prompt_template: Option<String>,
    pub daily_card_count: Option<u32>,
    pub daily_time_zone: Option<String>,
    pub daily_update_time: Option<String>,
    pub compression_level: Option<String>,
}

#[derive(Deserialize)]
pub struct RegenerateTopicIconReq {
    pub id: i64,
}

#[derive(Serialize)]
pub struct RegenerateTopicIconRes {
    pub icon_id: String,
    pub topic_color: String,
}

#[derive(Deserialize)]
pub struct DeleteTopicReq {
    pub id: i64,
}

#[derive(Serialize, Deserialize)]
pub struct TipcardInfo {
    pub id: i64,
    pub topic_name: String,
    pub topic_icon: String,
    pub topic_color: String,
    pub title: String,
    pub full_content: String,
    pub compressed_content: String,
    pub image_data: Vec<String>,
    pub created_at: String,
    pub tipcard_type: String,
    pub status: String,
    pub next_review_at: String,
    pub repeat_count: u32,
    pub pinned: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FlowCardInfo {
    pub id: i64,
    pub topic_name: String,
    pub topic_icon: String,
    pub topic_color: String,
    pub title: String,
    pub compressed_content: String,
    pub created_at: String,
    pub tipcard_type: String,
    pub status: String,
    pub next_review_at: String,
    pub repeat_count: u32,
    pub pinned: bool,
    pub image_count: i64,
    pub thumbnail_urls: Vec<String>,
}

#[derive(Serialize)]
pub struct FlowCardPage {
    pub cards: Vec<FlowCardInfo>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Serialize)]
pub struct FlowCardDetail {
    #[serde(flatten)]
    pub card: TipcardInfo,
    pub image_urls: Vec<String>,
}

#[derive(Default, Deserialize)]
pub struct FlowCardsQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Default, Deserialize)]
pub struct ListTipcardsQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub topic: Option<String>,
    pub tipcard_type: Option<String>,
}
