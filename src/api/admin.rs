use chrono::Utc;

use crate::{
    db::repositories::{tipcards, topics},
    AppState,
};

use super::{pb, types::ApiResult};

pub(crate) async fn list_admin_topics_pb(
    state: &AppState,
    user_id: &str,
) -> ApiResult<pb::AdminTopics> {
    let rows = topics::list_admin(&state.db, user_id)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(pb::AdminTopics {
        topics: rows
            .into_iter()
            .map(|row| pb::AdminTopic {
                id: row.id,
                name: row.name,
                prompt_template: row.prompt_template.unwrap_or_default(),
                daily_card_count: row.daily_card_count.unwrap_or(1).max(1) as u32,
                daily_time_zone: row.daily_time_zone.unwrap_or_default(),
                daily_update_time: row.daily_update_time.unwrap_or_default(),
                compression_level: row.compression_level.unwrap_or_default(),
            })
            .collect(),
    })
}

pub(crate) async fn list_tipcards_pb(state: &AppState, user_id: &str) -> ApiResult<pb::Tipcards> {
    let rows = tipcards::list_admin(&state.db, user_id)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(pb::Tipcards {
        cards: rows
            .into_iter()
            .map(|row| pb::TipcardInfo {
                id: row.id,
                topic_name: row.topic_name,
                full_content: row.full_content,
                compressed_content: row.compressed_content,
                created_at: row.created_at,
                tipcard_type: row.tipcard_type,
                status: row.status,
                next_review_at: row.next_review_at,
                repeat_count: serde_json::from_str::<serde_json::Value>(&row.state_data)
                    .ok()
                    .and_then(|value| value.get("repeats").and_then(|repeats| repeats.as_u64()))
                    .unwrap_or(0) as u32,
                pinned: row.pinned,
            })
            .collect(),
    })
}

pub(crate) async fn app_summary_pb(state: &AppState, user_id: &str) -> ApiResult<pb::AppSummary> {
    let summary = topics::app_summary(&state.db, user_id, Utc::now())
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(pb::AppSummary {
        topics: summary.topics,
        total_cards: summary.total_cards,
        due_cards: summary.due_cards,
        active_cards: summary.active_cards,
    })
}

pub(crate) async fn app_topics_pb(state: &AppState, user_id: &str) -> ApiResult<pb::AppTopics> {
    let rows = topics::list_app_topics(&state.db, user_id, Utc::now())
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(pb::AppTopics {
        topics: rows
            .into_iter()
            .map(|row| pb::AppTopicInfo {
                id: row.id,
                name: row.name,
                tipcard_type: row.tipcard_type,
                prompt_template: row.prompt_template,
                daily_card_count: row.daily_card_count,
                daily_time_zone: row.daily_time_zone,
                daily_update_time: row.daily_update_time,
                compression_level: row.compression_level,
                total_cards: row.total_cards,
                due_cards: row.due_cards,
                completed_cards: row.completed_cards,
            })
            .collect(),
    })
}
