use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

use crate::AppState;
use crate::dashboard::util::current_user;
use crate::services::documents::DocumentService;

#[derive(Deserialize)]
pub struct AddDocumentReq {
    #[serde(default)]
    pub topic_ids: Vec<i64>,
    pub source_type: String,
    pub title: String,
    pub url: Option<String>,
    pub content: String,
}

#[derive(Serialize)]
pub struct DocumentInfo {
    pub id: i64,
    pub topic_ids: Vec<i64>,
    pub source_type: String,
    pub title: String,
    pub url: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct DocumentDetail {
    pub id: i64,
    pub topic_ids: Vec<i64>,
    pub source_type: String,
    pub title: String,
    pub url: Option<String>,
    pub content: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct UploadDocumentReq {
    pub filename: String,
    pub title: Option<String>,
    pub data_url: String,
    #[serde(default)]
    pub topic_ids: Vec<i64>,
}

#[derive(Deserialize)]
pub struct DeleteDocumentReq {
    pub id: i64,
}

#[derive(Deserialize)]
pub struct AttachDocumentTopicReq {
    pub topic_id: i64,
}

#[derive(Deserialize)]
pub struct ExploreLinkReq {
    pub url: String,
}

pub async fn explore_link(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<ExploreLinkReq>,
) -> Result<Json<Vec<crate::services::documents::ExploredLink>>, (StatusCode, String)> {
    current_user(&state, &session).await?;
    DocumentService::explore_link(&req.url)
        .await
        .map(Json)
        .map_err(|err| err.into_status_body())
}

pub async fn list_documents(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Json<Vec<DocumentInfo>>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let rows = DocumentService::list_documents(&state, &user.id, None)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(
        rows.into_iter()
            .map(|row| DocumentInfo {
                id: row.id,
                topic_ids: row.topic_ids,
                source_type: row.source_type,
                title: row.title,
                url: row.url,
                created_at: row.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

pub async fn get_document(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(id): Path<i64>,
) -> Result<Json<DocumentDetail>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let doc = DocumentService::get_document(&state, &user.id, id)
        .await
        .map_err(|err| err.into_status_body())?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Document not found".to_string()))?;
    Ok(Json(DocumentDetail {
        id: doc.id,
        topic_ids: doc.topic_ids,
        source_type: doc.source_type,
        title: doc.title,
        url: doc.url,
        content: doc.content,
        created_at: doc.created_at.to_rfc3339(),
    }))
}

pub async fn add_document(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<AddDocumentReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    DocumentService::add_document(
        &state,
        &user.id,
        &req.topic_ids,
        &req.source_type,
        &req.title,
        req.url.as_deref(),
        &req.content,
    )
    .await
    .map_err(|err| err.into_status_body())?;
    Ok(Json(()))
}

pub async fn upload_document(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<UploadDocumentReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;

    // Parse the data URL: data:<mime>;base64,<payload>
    let (mime_type, payload) = req
        .data_url
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(','))
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "Invalid data URL format".to_string(),
            )
        })
        .map(|(header, payload)| {
            let mime = header
                .split(';')
                .next()
                .unwrap_or("application/octet-stream")
                .to_string();
            (mime, payload)
        })?;

    let data = STANDARD
        .decode(payload)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid base64: {e}")))?;

    DocumentService::upload_document(
        &state,
        &user.id,
        &req.topic_ids,
        &req.filename,
        &mime_type,
        req.title.as_deref(),
        &data,
    )
    .await
    .map_err(|err| err.into_status_body())?;
    Ok(Json(()))
}

pub async fn delete_document(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<DeleteDocumentReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    DocumentService::delete_document(&state, &user.id, req.id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(()))
}

pub async fn attach_document_topic(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(id): Path<i64>,
    Json(req): Json<AttachDocumentTopicReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    DocumentService::attach_document_topic(&state, &user.id, id, req.topic_id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(()))
}

pub async fn detach_document_topic(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path((id, topic_id)): Path<(i64, i64)>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    DocumentService::detach_document_topic(&state, &user.id, id, topic_id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(()))
}

#[derive(Deserialize)]
pub struct AddPoolImageReq {
    pub image_data: String,
    /// Fallback name if vision annotation is unavailable or fails.
    pub name: String,
}

#[derive(Serialize)]
pub struct PoolImageInfo {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct DeletePoolImageReq {
    pub id: i64,
}

#[derive(Deserialize)]
pub struct RenamePoolImageReq {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct RemovePoolImageTagReq {
    pub id: i64,
    pub tag: String,
}

pub async fn list_pool_images(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Json<Vec<PoolImageInfo>>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let rows = DocumentService::list_pool_images(&state, &user.id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(
        rows.into_iter()
            .map(|row| PoolImageInfo {
                id: row.id,
                name: row.name,
                description: row.description,
                tags: crate::llm::tags_from_json(&row.tags),
                created_at: row.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

pub async fn add_pool_image(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<AddPoolImageReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    DocumentService::add_pool_image(&state, &user.id, &req.image_data, &req.name, None)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(()))
}

pub async fn delete_pool_image(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<DeletePoolImageReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    DocumentService::delete_pool_image(&state, &user.id, req.id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(()))
}

pub async fn rename_pool_image(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<RenamePoolImageReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    DocumentService::rename_pool_image(
        &state,
        &user.id,
        req.id,
        &req.name,
        req.description.as_deref(),
    )
    .await
    .map_err(|err| err.into_status_body())?;
    Ok(Json(()))
}

pub async fn remove_pool_image_tag(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<RemovePoolImageTagReq>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    DocumentService::remove_pool_image_tag(&state, &user.id, req.id, &req.tag)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(()))
}
