use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use tower_sessions::Session;

use crate::AppState;
use crate::dashboard::response::{
    DeleteTipcardReq, FlowCardDetail, FlowCardInfo, FlowCardPage, FlowCardsQuery,
    ListTipcardsQuery, PinTipcardReq, TipcardInfo,
};
use crate::dashboard::util::{current_user, image_url, optional_user, parse_flow_cursor};
use crate::services::tipcards::{TipcardFilter, TipcardService};

pub async fn pin_tipcard(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<PinTipcardReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    if let Some(pinned) = req.pinned {
        TipcardService::set_pinned(&state, &user.id, req.id, pinned)
            .await
            .map_err(|err| err.into_status_body())?;
    }
    if let Some(image_data) = req.image_data {
        TipcardService::set_images(&state, &user.id, req.id, image_data).await?;
    }
    Ok(Json(()))
}

pub async fn delete_tipcard(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<DeleteTipcardReq>,
) -> StatusCode {
    let user = match optional_user(&state, &session).await {
        Some(user) => user,
        None => return StatusCode::UNAUTHORIZED,
    };
    match TipcardService::delete(&state, &user.id, req.id).await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn list_tipcards(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<ListTipcardsQuery>,
) -> Json<Vec<TipcardInfo>> {
    let user = match optional_user(&state, &session).await {
        Some(user) => user,
        None => return Json(Vec::new()),
    };
    let cards = TipcardService::list_tipcards(
        &state,
        &user.id,
        TipcardFilter {
            q: query.q,
            status: query.status,
            topic: query.topic,
            tipcard_type: query.tipcard_type,
        },
    )
    .await
    .unwrap_or_default();
    let card_ids: Vec<i64> = cards.iter().map(|card| card.id).collect();
    let images_map = TipcardService::list_images_for_cards(&state, &user.id, &card_ids)
        .await
        .unwrap_or_default();
    let cards = cards
        .into_iter()
        .map(|r| TipcardInfo {
            id: r.id,
            topic_name: r.topic_name,
            topic_icon: r.topic_icon,
            topic_color: r.topic_color,
            title: r.title,
            full_content: r.full_content,
            compressed_content: r.compressed_content,
            image_data: images_map
                .get(&r.id)
                .map(|images| images.iter().map(|image| image_url(image.id)).collect())
                .unwrap_or_default(),
            created_at: r.created_at,
            tipcard_type: r.tipcard_type,
            status: r.status,
            next_review_at: r.next_review_at,
            repeat_count: r.repeats,
            pinned: r.pinned,
        })
        .collect();

    Json(cards)
}

pub async fn flow_cards(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<FlowCardsQuery>,
) -> Result<Json<FlowCardPage>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let limit = query.limit.unwrap_or(48).clamp(1, 100);
    let cursor = query.cursor.as_deref().and_then(parse_flow_cursor);
    let rows = TipcardService::list_flow_cards(&state, &user.id, cursor, limit + 1)
        .await
        .map_err(|err| err.into_status_body())?;
    let has_more = rows.len() > limit as usize;
    let rows: Vec<_> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        rows.last().map(|row| {
            format!(
                "{}|{}|{}",
                if row.pinned { 1 } else { 0 },
                row.created_at,
                row.id
            )
        })
    } else {
        None
    };
    let card_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let images_map = TipcardService::list_images_for_cards(&state, &user.id, &card_ids)
        .await
        .map_err(|err| err.into_status_body())?;
    let mut cards = Vec::new();
    for row in rows {
        cards.push(FlowCardInfo {
            id: row.id,
            topic_name: row.topic_name,
            topic_icon: row.topic_icon,
            topic_color: row.topic_color,
            title: row.title,
            full_content: row.full_content,
            compressed_content: row.compressed_content,
            created_at: row.created_at,
            tipcard_type: row.tipcard_type,
            status: row.status,
            next_review_at: row.next_review_at,
            repeat_count: row.repeats,
            pinned: row.pinned,
            image_count: row.image_count,
            thumbnail_urls: images_map
                .get(&row.id)
                .map(|imgs| {
                    imgs.iter()
                        .take(4)
                        .map(|image| image_url(image.id))
                        .collect()
                })
                .unwrap_or_default(),
        });
    }
    Ok(Json(FlowCardPage {
        cards,
        next_cursor,
        has_more,
    }))
}

pub async fn flow_card_detail(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(id): Path<i64>,
) -> Result<Json<FlowCardDetail>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let (row, images) = TipcardService::tipcard_detail(&state, &user.id, id)
        .await
        .map_err(|err| err.into_status_body())?;
    let image_urls: Vec<String> = images.iter().map(|image| image_url(image.id)).collect();
    Ok(Json(FlowCardDetail {
        card: TipcardInfo {
            id: row.id,
            topic_name: row.topic_name,
            topic_icon: row.topic_icon,
            topic_color: row.topic_color,
            title: row.title,
            full_content: row.full_content,
            compressed_content: row.compressed_content,
            image_data: image_urls.clone(),
            created_at: row.created_at,
            tipcard_type: row.tipcard_type,
            status: row.status,
            next_review_at: row.next_review_at,
            repeat_count: row.repeats,
            pinned: row.pinned,
        },
        image_urls,
    }))
}
