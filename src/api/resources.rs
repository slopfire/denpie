use axum::http::StatusCode;

use crate::{
    AppState,
    api::pb,
    services::{documents::DocumentService, tipcards::TipcardService, tips::TipService},
    types::ContinueDailyReviewRequest,
};

use super::types::ApiResult;

pub(crate) fn api_info() -> pb::ApiInfo {
    pb::ApiInfo {
        api_version: "v1".to_string(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        build_sha: option_env!("DENPIE_BUILD_SHA")
            .unwrap_or("unknown")
            .to_string(),
        capabilities: vec![
            "bearer_auth".to_string(),
            "structured_errors".to_string(),
            "request_ids".to_string(),
            "durable_idempotency".to_string(),
            "flow_card_pagination".to_string(),
            "tipcard_detail".to_string(),
            "document_detail".to_string(),
            "document_explore".to_string(),
            "daily_review_continuation".to_string(),
            "vision_diagnostics".to_string(),
            "authenticated_image_downloads".to_string(),
        ],
    }
}

pub(crate) async fn list_flow_cards(
    state: &AppState,
    user_id: &str,
    req: pb::ListFlowCardsRequest,
) -> ApiResult<pb::FlowCardPage> {
    let page_size = if req.page_size == 0 {
        48
    } else {
        i64::from(req.page_size.clamp(1, 100))
    };
    let cursor = parse_page_token(&req.page_token)?;
    let rows = TipcardService::list_flow_cards(state, user_id, cursor, page_size + 1)
        .await
        .map_err(|err| err.into_status_body())?;
    let has_more = rows.len() > page_size as usize;
    let rows: Vec<_> = rows.into_iter().take(page_size as usize).collect();
    let card_ids: Vec<_> = rows.iter().map(|row| row.id).collect();
    let images = TipcardService::list_images_for_cards(state, user_id, &card_ids)
        .await
        .map_err(|err| err.into_status_body())?;
    let next_page_token = if has_more {
        rows.last()
            .map(|row| encode_page_token(row.pinned, &row.created_at, row.id))
            .unwrap_or_default()
    } else {
        String::new()
    };

    let cards = rows
        .into_iter()
        .map(|row| {
            let card_images = images.get(&row.id).cloned().unwrap_or_default();
            flow_card_to_pb(row, card_images)
        })
        .collect();

    Ok(pb::FlowCardPage {
        cards,
        next_page_token,
        has_more,
    })
}

pub(crate) async fn get_tipcard(
    state: &AppState,
    user_id: &str,
    id: i64,
) -> ApiResult<pb::TipcardDetail> {
    let (card, images) = TipcardService::tipcard_detail(state, user_id, id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(pb::TipcardDetail {
        card: Some(pb::FlowCardInfo {
            id: card.id,
            topic_name: card.topic_name,
            topic_icon: card.topic_icon,
            topic_color: card.topic_color,
            title: card.title,
            full_content: card.full_content,
            compressed_content: card.compressed_content,
            created_at: card.created_at,
            tipcard_type: card.tipcard_type,
            status: card.status,
            next_review_at: card.next_review_at,
            repeat_count: card.repeats,
            pinned: card.pinned,
            pending_count: 0,
            images: images.into_iter().map(image_to_pb).collect(),
        }),
    })
}

pub(crate) async fn get_document(
    state: &AppState,
    user_id: &str,
    id: i64,
) -> ApiResult<pb::DocumentDetail> {
    let document = DocumentService::get_document(state, user_id, id)
        .await
        .map_err(|err| err.into_status_body())?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Document not found".to_string()))?;
    Ok(pb::DocumentDetail {
        id: document.id,
        topic_ids: document.topic_ids,
        source_type: document.source_type,
        title: document.title,
        url: document.url.unwrap_or_default(),
        content: document.content,
        created_at: document.created_at.to_rfc3339(),
    })
}

pub(crate) async fn upload_document(
    state: &AppState,
    user_id: &str,
    req: pb::UploadDocumentRequest,
) -> ApiResult<pb::DocumentDetail> {
    if req.data.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "data is required".to_string()));
    }
    let title = nonempty(req.title);
    let id = DocumentService::upload_document(
        state,
        user_id,
        &req.topic_ids,
        &req.filename,
        &req.mime_type,
        title.as_deref(),
        &req.data,
    )
    .await
    .map_err(|err| err.into_status_body())?;
    get_document(state, user_id, id).await
}

pub(crate) fn pool_image_created(
    result: crate::services::documents::PoolImageAddResult,
) -> pb::PoolImageCreated {
    pb::PoolImageCreated {
        id: result.id,
        name: result.name,
        description: result.description.unwrap_or_default(),
        tags: result.tags,
        annotated: result.annotated,
        fallback_reason: result.fallback_reason.unwrap_or_default(),
        model: result.model.unwrap_or_default(),
        download_path: format!("/api/v1/pool-images/{}", result.id),
    }
}

pub(crate) async fn continue_daily_review(
    state: &AppState,
    user_id: &str,
    req: pb::ContinueDailyReviewRequest,
) -> ApiResult<pb::ContinueDailyReviewResponse> {
    let result = TipService::continue_daily_review(
        state,
        user_id,
        ContinueDailyReviewRequest {
            topics: req.topics.join(","),
            tipcard_type: nonempty(req.tipcard_type),
        },
    )
    .await?;
    Ok(pb::ContinueDailyReviewResponse {
        available_cards: result.refreshed_cards,
    })
}

pub(crate) async fn explore_link(url: &str) -> ApiResult<pb::ExploredLinks> {
    let links = DocumentService::explore_link(url)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(pb::ExploredLinks {
        links: links
            .into_iter()
            .map(|link| pb::ExploredLink {
                title: link.title,
                url: link.url,
            })
            .collect(),
    })
}

pub(crate) async fn test_vision_model(
    state: &AppState,
    user_id: &str,
) -> ApiResult<pb::VisionModelTest> {
    let result = DocumentService::test_vision_model(state, user_id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(pb::VisionModelTest {
        ok: result.ok,
        model: result.model,
        message: result.message,
    })
}

fn flow_card_to_pb(
    card: crate::db::repositories::tipcards::FlowCardRecord,
    images: Vec<crate::db::repositories::tipcards::TipcardImageRecord>,
) -> pb::FlowCardInfo {
    pb::FlowCardInfo {
        id: card.id,
        topic_name: card.topic_name,
        topic_icon: card.topic_icon,
        topic_color: card.topic_color,
        title: card.title,
        full_content: card.full_content,
        compressed_content: card.compressed_content,
        created_at: card.created_at,
        tipcard_type: card.tipcard_type,
        status: card.status,
        next_review_at: card.next_review_at,
        repeat_count: card.repeats,
        pinned: card.pinned,
        pending_count: card.pending_count,
        images: images.into_iter().map(image_to_pb).collect(),
    }
}

fn image_to_pb(
    image: crate::db::repositories::tipcards::TipcardImageRecord,
) -> pb::TipcardImageInfo {
    pb::TipcardImageInfo {
        id: image.id,
        position: image.position,
        mime_type: image.mime_type,
        byte_size: image.byte_size,
        download_path: format!("/api/v1/tipcard-images/{}", image.id),
    }
}

fn parse_page_token(token: &str) -> ApiResult<Option<(i64, String, i64)>> {
    if token.trim().is_empty() {
        return Ok(None);
    }
    let mut parts = token.splitn(3, '|');
    let pinned = parts
        .next()
        .and_then(|value| value.parse().ok())
        .filter(|value| matches!(value, 0 | 1));
    let created_at = parts.next().filter(|value| !value.is_empty());
    let id = parts.next().and_then(|value| value.parse().ok());
    match (pinned, created_at, id) {
        (Some(pinned), Some(created_at), Some(id)) if id > 0 => {
            Ok(Some((pinned, created_at.to_string(), id)))
        }
        _ => Err((StatusCode::BAD_REQUEST, "Invalid page_token".to_string())),
    }
}

fn encode_page_token(pinned: bool, created_at: &str, id: i64) -> String {
    format!("{}|{created_at}|{id}", i64::from(pinned))
}

fn nonempty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{encode_page_token, parse_page_token};

    #[test]
    fn page_token_round_trip() {
        let token = encode_page_token(true, "2026-08-09 12:34:56+00", 42);
        assert_eq!(
            parse_page_token(&token).unwrap(),
            Some((1, "2026-08-09 12:34:56+00".to_string(), 42))
        );
    }

    #[test]
    fn malformed_page_token_is_rejected() {
        assert!(parse_page_token("bogus").is_err());
        assert!(parse_page_token("2|timestamp|42").is_err());
        assert!(parse_page_token("1|timestamp|0").is_err());
    }
}
