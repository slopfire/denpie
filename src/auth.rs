use axum::{
    extract::{Request, State, Json},
    middleware::Next,
    response::Response,
    http::StatusCode,
};
use std::sync::Arc;
use sha2::Digest;
use crate::AppState;
use tower_sessions::Session;

pub async fn verify_api_key(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = req.headers();
    let auth_header = headers.get("Authorization");

    if let Some(auth_value) = auth_header {
        let key = auth_value.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
        
        let mut hasher = sha2::Sha256::new();
        sha2::Digest::update(&mut hasher, key.as_bytes());
        let key_hash = hex::encode(hasher.finalize());

        let exists: Option<String> = sqlx::query_scalar!(
            "SELECT client_name FROM api_keys WHERE key_hash = ?",
            key_hash
        )
        .fetch_optional(&state.db)
        .await
        .map_err(|_: sqlx::Error| StatusCode::INTERNAL_SERVER_ERROR)?;

        if exists.is_some() {
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
    _state: State<Arc<AppState>>,
    session: Session,
    Json(req): Json<LoginReq>,
) -> Result<StatusCode, StatusCode> {
    let settings_str = std::fs::read_to_string("settings.yaml").unwrap_or_default();
    let settings: serde_yaml::Value = serde_yaml::from_str(&settings_str).unwrap_or(serde_yaml::Value::Mapping(Default::default()));
    let real_token = settings.get("admin_token").and_then(|v| v.as_str()).unwrap_or("");
    
    if req.admin_token == real_token && !real_token.is_empty() {
        session.insert("user", "admin").await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
