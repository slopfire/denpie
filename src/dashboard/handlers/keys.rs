use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use tower_sessions::Session;

use crate::AppState;
use crate::dashboard::response::{ApiKeyInfo, CreateKeyReq, DeleteKeyReq};
use crate::dashboard::util::optional_user;

pub async fn create_api_key(
    State(state): State<Arc<AppState>>,
    session: Session,
    req: Option<Json<CreateKeyReq>>,
) -> Json<String> {
    let user = match optional_user(&state, &session).await {
        Some(user) => user,
        None => return Json(String::new()),
    };
    let client_name = req
        .and_then(|Json(r)| r.client_name)
        .unwrap_or_else(|| "default_client".to_string());
    let api_key = state
        .api_keys
        .create(&user.id, Some(client_name))
        .await
        .unwrap_or_default();

    Json(api_key)
}

pub async fn list_api_keys(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Json<Vec<ApiKeyInfo>> {
    let user = match optional_user(&state, &session).await {
        Some(user) => user,
        None => return Json(Vec::new()),
    };
    let keys = state
        .api_keys
        .list(&user.id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| ApiKeyInfo {
            id: row.id,
            client_name: row.client_name,
            created_at: row.created_at,
        })
        .collect();

    Json(keys)
}

pub async fn delete_api_key(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<DeleteKeyReq>,
) -> StatusCode {
    let user = match optional_user(&state, &session).await {
        Some(user) => user,
        None => return StatusCode::UNAUTHORIZED,
    };
    if state.api_keys.delete(&user.id, req.id).await.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}
