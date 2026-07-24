//! Shared types and pure helpers for the grounding view.
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone, PartialEq)]
pub struct AppSummary {
    pub due_cards: i64,
    pub active_cards: i64,
    pub total_cards: i64,
    pub topics: i64,
}

#[derive(Deserialize, Clone, PartialEq)]
pub struct TokenSpend {
    pub daily: i64,
    pub monthly: i64,
    pub total: i64,
}

#[derive(Deserialize, Clone, PartialEq)]
pub struct AppTopicInfo {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
    pub icon_id: String,
    pub topic_color: String,
    pub prompt_template: String,
    pub total_cards: i64,
    pub due_cards: i64,
    pub pending_cards: i64,
    pub completed_cards: i64,
    pub daily_card_count: u32,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub compression_level: String,
    pub grounding_strategy: String,
    pub image_strategy: String,
}

#[derive(Serialize)]
pub(crate) struct UpdateTopicReq {
    pub(crate) id: i64,
    pub(crate) prompt_template: Option<String>,
    pub(crate) daily_card_count: Option<u32>,
    pub(crate) daily_time_zone: Option<String>,
    pub(crate) daily_update_time: Option<String>,
    pub(crate) compression_level: Option<String>,
    pub(crate) grounding_strategy: Option<String>,
    pub(crate) image_strategy: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct DeleteTopicReq {
    pub(crate) id: i64,
}

#[derive(Serialize)]
pub(crate) struct RegenerateTopicIconReq {
    pub(crate) id: i64,
}

#[derive(Deserialize)]
pub(crate) struct RegenerateTopicIconRes {
    pub(crate) icon_id: String,
    pub(crate) topic_color: String,
}

#[derive(Serialize)]
pub(crate) struct ForceDailyRefreshReq {
    pub(crate) topics: String,
    pub(crate) tipcard_type: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ForceDailyRefreshRes {
    pub(crate) refreshed_cards: u64,
}

pub(crate) fn title_from_url(url: &str) -> String {
    let path = url
        .split("://")
        .nth(1)
        .and_then(|rest| {
            let slash_idx = rest.find('/')?;
            Some(&rest[slash_idx..])
        })
        .unwrap_or(url);

    let last = path.split('/').rfind(|s| !s.is_empty()).unwrap_or("");

    if last.is_empty() {
        return url.to_string();
    }

    last.replace(['-', '_'], " ")
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn set_fullscreen_body_class(fullscreen: bool) {
    let Some(body) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
    else {
        return;
    };
    let _ = body
        .class_list()
        .toggle_with_force("has-fullscreen-card", fullscreen);
}
