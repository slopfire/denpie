use crate::{AppState, api::pb, services::documents::DocumentService};

use super::types::ApiResult;

pub(crate) async fn add_document(
    state: &AppState,
    user_id: &str,
    req: pb::AddDocumentRequest,
) -> ApiResult<i64> {
    let mut topic_ids = req.topic_ids;
    if let Some(topic_id) = parse_topic_id_opt(&req.topic_id_opt)? {
        topic_ids.push(topic_id);
    }
    DocumentService::add_document(
        state,
        user_id,
        &topic_ids,
        &req.source_type,
        &req.title,
        empty_as_none(&req.url),
        &req.content,
    )
    .await
    .map_err(|err| err.into_status_body())
}

pub(crate) async fn list_documents(state: &AppState, user_id: &str) -> ApiResult<pb::Documents> {
    let rows = DocumentService::list_documents(state, user_id, None)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(pb::Documents {
        docs: rows
            .into_iter()
            .map(|row| pb::DocumentInfo {
                id: row.id,
                topic_id: row.topic_ids.first().copied().unwrap_or(0),
                source_type: row.source_type,
                title: row.title,
                url: row.url.unwrap_or_default(),
                created_at: row.created_at.to_rfc3339(),
                topic_ids: row.topic_ids,
            })
            .collect(),
    })
}

pub(crate) async fn delete_document(state: &AppState, user_id: &str, id: i64) -> ApiResult<()> {
    DocumentService::delete_document(state, user_id, id)
        .await
        .map_err(|err| err.into_status_body())
}

pub(crate) async fn attach_document_topic(
    state: &AppState,
    user_id: &str,
    req: pb::AttachDocumentTopicRequest,
) -> ApiResult<()> {
    DocumentService::attach_document_topic(state, user_id, req.document_id, req.topic_id)
        .await
        .map_err(|err| err.into_status_body())
}

pub(crate) async fn detach_document_topic(
    state: &AppState,
    user_id: &str,
    req: pb::AttachDocumentTopicRequest,
) -> ApiResult<()> {
    DocumentService::detach_document_topic(state, user_id, req.document_id, req.topic_id)
        .await
        .map_err(|err| err.into_status_body())
}

pub(crate) async fn add_pool_image(
    state: &AppState,
    user_id: &str,
    req: pb::AddPoolImageRequest,
) -> ApiResult<crate::services::documents::PoolImageAddResult> {
    DocumentService::add_pool_image(
        state,
        user_id,
        &req.image_data,
        &req.name,
        empty_as_none(&req.description),
    )
    .await
    .map_err(|err| err.into_status_body())
}

pub(crate) async fn list_pool_images(state: &AppState, user_id: &str) -> ApiResult<pb::PoolImages> {
    let rows = DocumentService::list_pool_images(state, user_id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(pb::PoolImages {
        images: rows
            .into_iter()
            .map(|row| pb::PoolImageInfo {
                id: row.id,
                name: row.name,
                description: row.description.unwrap_or_default(),
                created_at: row.created_at.to_rfc3339(),
                tags: crate::llm::tags_from_json(&row.tags),
            })
            .collect(),
    })
}

pub(crate) async fn delete_pool_image(state: &AppState, user_id: &str, id: i64) -> ApiResult<()> {
    DocumentService::delete_pool_image(state, user_id, id)
        .await
        .map_err(|err| err.into_status_body())
}

/// Parse the legacy optional topic id. Empty string or "0" means unassigned.
fn parse_topic_id_opt(value: &str) -> ApiResult<Option<i64>> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "0" {
        return Ok(None);
    }
    let parsed: i64 = trimmed.parse().map_err(|_| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            "invalid topic_id".to_string(),
        )
    })?;
    Ok(Some(parsed))
}

fn empty_as_none(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(value)
    }
}
