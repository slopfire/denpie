use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use tower_sessions::Session;

use crate::AppState;
use crate::dashboard::response::{
    AppSummary, AppTopicInfo, DeleteTopicReq, RegenerateTopicIconReq, RegenerateTopicIconRes,
    TopicInfo, UpdateTopicReq,
};
use crate::dashboard::util::{current_user, optional_user};
use crate::services::topics::{AdminTopicInfo, TopicService, UpdateTopicSettings};

pub async fn list_topics(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Json<Vec<TopicInfo>> {
    let user = match optional_user(&state, &session).await {
        Some(user) => user,
        None => return Json(Vec::new()),
    };
    let topics = TopicService::list_admin_topics(&state, &user.id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r: AdminTopicInfo| TopicInfo {
            id: r.id,
            name: r.name,
            tipcard_type: r.tipcard_type,
            icon_id: r.icon_id,
            topic_color: r.topic_color,
            prompt_template: r.prompt_template,
            daily_card_count: r.daily_card_count,
            daily_time_zone: r.daily_time_zone,
            daily_update_time: r.daily_update_time,
            compression_level: r.compression_level,
        })
        .collect();
    Json(topics)
}

pub async fn app_summary(State(state): State<Arc<AppState>>, session: Session) -> Json<AppSummary> {
    let user = match optional_user(&state, &session).await {
        Some(user) => user,
        None => {
            return Json(AppSummary {
                topics: 0,
                total_cards: 0,
                due_cards: 0,
                active_cards: 0,
            });
        }
    };
    match TopicService::app_summary(&state, &user.id).await {
        Ok(summary) => Json(AppSummary {
            topics: summary.topics,
            total_cards: summary.total_cards,
            due_cards: summary.due_cards,
            active_cards: summary.active_cards,
        }),
        Err(_) => Json(AppSummary {
            topics: 0,
            total_cards: 0,
            due_cards: 0,
            active_cards: 0,
        }),
    }
}

pub async fn app_topics(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Json<Vec<AppTopicInfo>> {
    let user = match optional_user(&state, &session).await {
        Some(user) => user,
        None => return Json(Vec::new()),
    };
    Json(
        TopicService::list_app_topics(&state, &user.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| AppTopicInfo {
                id: r.id,
                name: r.name,
                tipcard_type: r.tipcard_type,
                icon_id: r.icon_id,
                topic_color: r.topic_color,
                prompt_template: r.prompt_template,
                daily_card_count: r.daily_card_count,
                daily_time_zone: r.daily_time_zone,
                daily_update_time: r.daily_update_time,
                compression_level: r.compression_level,
                total_cards: r.total_cards,
                due_cards: r.due_cards,
                completed_cards: r.completed_cards,
            })
            .collect(),
    )
}

pub async fn update_topic(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<UpdateTopicReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    TopicService::update_topic_settings(
        &state,
        &user.id,
        req.id,
        UpdateTopicSettings {
            prompt_template: req.prompt_template,
            daily_card_count: req.daily_card_count,
            daily_time_zone: req.daily_time_zone,
            daily_update_time: req.daily_update_time,
            compression_level: req.compression_level,
        },
    )
    .await
    .map_err(|err| err.into_status_body())?;

    Ok(Json(()))
}

pub async fn regenerate_topic_icon(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<RegenerateTopicIconReq>,
) -> Result<Json<RegenerateTopicIconRes>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let update = TopicService::regenerate_topic_icon(&state, &user.id, req.id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(RegenerateTopicIconRes {
        icon_id: update.icon_id,
        topic_color: update.topic_color,
    }))
}

pub async fn delete_topic(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<DeleteTopicReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    TopicService::delete_topic_by_id(&state, &user.id, req.id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(()))
}
