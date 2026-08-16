use axum::http::StatusCode;

use crate::{
    AppState,
    api::pb,
    db::repositories::{documents as document_repo, topics as topic_repo},
    services::{documents::DocumentService, tipcards::TipcardService, tips::TipService},
    types::ContinueDailyReviewRequest,
};

use std::collections::HashMap;

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
            "atomic_review_advance".to_string(),
            "durable_image_enrichment".to_string(),
        ],
    }
}

pub async fn list_flow_cards(
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

    let sources_by_topic = card_sources_by_topic_name(
        &topic_repo::list_admin(&state.db, user_id)
            .await
            .map_err(|err| err.into_status_body())?,
        &document_repo::list_document_topic_links(&state.db, user_id)
            .await
            .map_err(|err| err.into_status_body())?,
    );

    let cards = rows
        .into_iter()
        .map(|row| {
            let card_images = images.get(&row.id).cloned().unwrap_or_default();
            let sources = sources_by_topic
                .get(&row.topic_name)
                .cloned()
                .unwrap_or_default();
            flow_card_to_pb(row, card_images, sources)
        })
        .collect();

    Ok(pb::FlowCardPage {
        cards,
        next_page_token,
        has_more,
    })
}

pub async fn get_tipcard(state: &AppState, user_id: &str, id: i64) -> ApiResult<pb::TipcardDetail> {
    let (card, images) = TipcardService::tipcard_detail(state, user_id, id)
        .await
        .map_err(|err| err.into_status_body())?;
    let sources = card_sources_by_topic_name(
        &topic_repo::list_admin(&state.db, user_id)
            .await
            .map_err(|err| err.into_status_body())?,
        &document_repo::list_document_topic_links(&state.db, user_id)
            .await
            .map_err(|err| err.into_status_body())?,
    )
    .remove(&card.topic_name)
    .unwrap_or_default();
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
            sources,
        }),
    })
}

pub(crate) async fn review_and_advance(
    state: &AppState,
    user_id: &str,
    req: pb::ReviewAndAdvanceRequest,
    grade: u8,
    action: &str,
) -> ApiResult<pb::ReviewAndAdvanceResponse> {
    let result = TipService::review_and_advance(state, user_id, req.card_id, grade, action).await?;
    let next_card = match result.next_card_id {
        Some(id) => {
            let mut card = get_tipcard(state, user_id, id).await?.card;
            if let Some(card) = card.as_mut() {
                card.pending_count = result.pending_count.max(0);
            }
            card
        }
        None => None,
    };
    Ok(pb::ReviewAndAdvanceResponse {
        reviewed_card_id: req.card_id,
        next_card,
        daily_complete: result.daily_complete,
        pending_count: result.pending_count.max(0) as u32,
        refill_scheduled: result.refill_scheduled,
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
    Ok(continue_daily_review_to_pb(result))
}

fn continue_daily_review_to_pb(
    result: crate::services::tips::ContinueDailyReviewResult,
) -> pb::ContinueDailyReviewResponse {
    pb::ContinueDailyReviewResponse {
        available_cards: result.response.available_cards,
        active_card_id: Some(result.active_card_id),
        pending_count: result.pending_count,
    }
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
    sources: Vec<pb::CardSource>,
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
        sources,
    }
}

/// Map topic name → grounding sources (documents/links assigned to that topic).
/// Topic names are unique per user (`UNIQUE(user_id, name)`), so they are a
/// stable key between a card and its topic's assigned sources.
fn card_sources_by_topic_name(
    topics: &[topic_repo::TopicRecord],
    links: &[document_repo::DocumentTopicLink],
) -> HashMap<String, Vec<pb::CardSource>> {
    let name_by_id: HashMap<i64, &str> = topics
        .iter()
        .map(|topic| (topic.id, topic.name.as_str()))
        .collect();
    let mut sources: HashMap<String, Vec<pb::CardSource>> = HashMap::new();
    for link in links {
        let Some(topic_name) = name_by_id.get(&link.topic_id) else {
            continue;
        };
        sources
            .entry((*topic_name).to_string())
            .or_default()
            .push(pb::CardSource {
                document_id: link.document_id,
                source_type: link.source_type.clone(),
                title: link.title.clone(),
                url: link.url.clone(),
            });
    }
    sources
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
    use super::{
        card_sources_by_topic_name, continue_daily_review_to_pb, encode_page_token,
        parse_page_token,
    };
    use crate::db::repositories::{documents::DocumentTopicLink, topics::TopicRecord};
    use crate::services::tips::ContinueDailyReviewResult;
    use crate::types::{ForceDailyRefreshOutcome, ForceDailyRefreshResponse};

    #[test]
    fn continue_daily_review_maps_the_active_slot_card_id() {
        let mapped = continue_daily_review_to_pb(ContinueDailyReviewResult {
            response: ForceDailyRefreshResponse {
                refreshed_cards: 1,
                outcome: ForceDailyRefreshOutcome::CardAvailable,
                available_cards: 1,
                generated_cards: 5,
            },
            active_card_id: 42,
            pending_count: 4,
        });
        assert_eq!(mapped.available_cards, 1);
        assert_eq!(mapped.active_card_id, Some(42));
        assert_eq!(mapped.pending_count, 4);
    }

    fn topic(id: i64, name: &str) -> TopicRecord {
        TopicRecord {
            id,
            name: name.to_string(),
            tipcard_type: "repeatable_tip".to_string(),
            prompt_template: None,
            daily_card_count: None,
            daily_time_zone: None,
            daily_update_time: None,
            compression_level: None,
            icon_id: None,
            color_hue: None,
            grounding_strategy: None,
            image_strategy: None,
        }
    }

    fn link(
        document_id: i64,
        source_type: &str,
        title: &str,
        url: Option<&str>,
        topic_id: i64,
    ) -> DocumentTopicLink {
        DocumentTopicLink {
            document_id,
            source_type: source_type.to_string(),
            title: title.to_string(),
            url: url.map(str::to_string),
            topic_id,
        }
    }

    #[test]
    fn sources_group_by_topic_name() {
        let topics = vec![topic(1, "Rust"), topic(2, "Go")];
        let links = vec![
            link(
                10,
                "link",
                "Rust book",
                Some("https://doc.rust-lang.org/book"),
                1,
            ),
            link(11, "document", "Ownership notes", None, 1),
            link(12, "link", "Go spec", Some("https://go.dev/ref/spec"), 2),
        ];
        let map = card_sources_by_topic_name(&topics, &links);

        let rust = map.get("Rust").unwrap();
        assert_eq!(rust.len(), 2);
        assert!(rust.iter().any(|s| s.title == "Rust book"
            && s.url.as_deref() == Some("https://doc.rust-lang.org/book")));
        assert!(rust.iter().any(|s| s.title == "Ownership notes"
            && s.source_type == "document"
            && s.url.is_none()));
        assert_eq!(map.get("Go").unwrap().len(), 1);
        assert_eq!(map.get("Go").unwrap()[0].document_id, 12);
    }

    #[test]
    fn links_without_known_topic_are_dropped() {
        let topics = vec![topic(1, "Rust")];
        let links = vec![
            link(10, "link", "Assigned", Some("https://example.com/a"), 1),
            // Stale assignment to a deleted topic.
            link(11, "link", "Orphaned", Some("https://example.com/b"), 99),
        ];
        let map = card_sources_by_topic_name(&topics, &links);
        assert_eq!(map.len(), 1);
        assert_eq!(map["Rust"].len(), 1);
        assert_eq!(map["Rust"][0].document_id, 10);
    }

    #[test]
    fn shared_document_appears_for_every_assigned_topic() {
        let topics = vec![topic(1, "Rust"), topic(2, "Borrowing")];
        let links = vec![
            link(7, "document", "Shared source", None, 1),
            link(7, "document", "Shared source", None, 2),
        ];
        let map = card_sources_by_topic_name(&topics, &links);
        assert_eq!(map["Rust"].len(), 1);
        assert_eq!(map["Borrowing"].len(), 1);
        assert_eq!(map["Rust"][0].document_id, 7);
        assert_eq!(map["Rust"][0].document_id, map["Borrowing"][0].document_id);
    }

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
