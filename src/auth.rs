use crate::AppState;
use axum::{
    extract::{Json, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use tower_sessions::Session;

#[allow(dead_code)]
pub async fn verify_api_key(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = req.headers();
    let auth_header = headers.get("Authorization");

    if let Some(auth_value) = auth_header {
        let key = auth_value.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;

        if state.api_keys.verify(key).await.is_ok() {
            Ok(next.run(req).await)
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[derive(serde::Deserialize)]
pub struct LoginReq {
    pub admin_token: String,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<LoginReq>,
) -> Result<StatusCode, StatusCode> {
    let settings = state
        .settings
        .get_settings()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let real_token = settings.admin_token;

    if req.admin_token == real_token && !real_token.is_empty() {
        session
            .insert("user", "admin")
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(StatusCode::OK)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub async fn require_session(
    session: Session,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let user: Option<String> = session.get("user").await.unwrap_or(None);
    if user.is_some() {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
