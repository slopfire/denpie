//! Shared types and pure helpers for the grounding view.
use crate::api_v1;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone, PartialEq)]
pub struct AppSummary {
    pub due_cards: i64,
    pub active_cards: i64,
    pub total_cards: i64,
    pub topics: i64,
}

impl From<api_v1::AppSummaryView> for AppSummary {
    fn from(s: api_v1::AppSummaryView) -> Self {
        Self {
            due_cards: s.due_cards,
            active_cards: s.active_cards,
            total_cards: s.total_cards,
            topics: s.topics,
        }
    }
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

impl From<api_v1::AppTopicView> for AppTopicInfo {
    fn from(t: api_v1::AppTopicView) -> Self {
        Self {
            id: t.id,
            name: t.name,
            tipcard_type: t.tipcard_type,
            icon_id: t.icon_id,
            topic_color: t.topic_color,
            prompt_template: t.prompt_template,
            total_cards: t.total_cards,
            due_cards: t.due_cards,
            pending_cards: t.pending_cards,
            completed_cards: t.completed_cards,
            daily_card_count: t.daily_card_count,
            daily_time_zone: t.daily_time_zone,
            daily_update_time: t.daily_update_time,
            compression_level: t.compression_level,
            grounding_strategy: t.grounding_strategy,
            image_strategy: t.image_strategy,
        }
    }
}

/// Session JSON — no v1 op for icon suggestions.
#[derive(Serialize)]
pub(crate) struct SuggestTopicIconsReq {
    pub(crate) id: i64,
    pub(crate) excluded_icons: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct SuggestTopicIconsRes {
    pub(crate) icons: Vec<String>,
}

/// Session JSON — no v1 op for set-icon.
#[derive(Serialize)]
pub(crate) struct SetTopicIconReq {
    pub(crate) id: i64,
    pub(crate) icon_id: String,
}

#[derive(Deserialize)]
pub(crate) struct SetTopicIconRes {
    pub(crate) icon_id: String,
}

pub(crate) fn icon_short_name(icon: &str) -> String {
    icon.rsplit(':').next().unwrap_or(icon).replace('-', " ")
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
