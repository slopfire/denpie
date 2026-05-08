use crate::AppState;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Json, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use rand::Rng;
use std::sync::Arc;
use tower_sessions::Session;

use crate::db::repositories::{user_settings, users};

#[derive(Clone, Debug, serde::Serialize)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub role: String,
}

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
    pub username: String,
    pub password: String,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<LoginReq>,
) -> Result<StatusCode, StatusCode> {
    let user = users::find_by_username(&state.db, req.username.trim())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let hash = user
        .password_hash
        .as_deref()
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let parsed_hash = PasswordHash::new(hash).map_err(|_| StatusCode::UNAUTHORIZED)?;
    Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed_hash)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    session
        .insert("user_id", user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn me(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Json<AuthUser>, (StatusCode, String)> {
    current_user(&state, &session).await.map(Json)
}

pub async fn logout(session: Session) -> Result<StatusCode, StatusCode> {
    session
        .delete()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

#[derive(serde::Deserialize)]
pub struct SetupReq {
    pub admin_token: String,
    pub username: String,
    pub password: String,
}

pub async fn setup(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<SetupReq>,
) -> Result<Json<AuthUser>, (StatusCode, String)> {
    let settings = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    if settings.admin_token.is_empty() || req.admin_token != settings.admin_token {
        return Err((StatusCode::UNAUTHORIZED, "Invalid setup token".to_string()));
    }
    if !users::setup_allowed(&state.db)
        .await
        .map_err(|err| err.into_status_body())?
    {
        return Err((StatusCode::CONFLICT, "Setup already complete".to_string()));
    }
    let username = req.username.trim();
    if username.is_empty() || req.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Username and password of at least 8 characters are required".to_string(),
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .to_string();
    let user_id = new_user_id();
    let role = if users::count(&state.db)
        .await
        .map_err(|err| err.into_status_body())?
        == 0
    {
        "admin"
    } else {
        "user"
    };

    let user = users::create(&state.db, &user_id, username, &password_hash, role)
        .await
        .map_err(|err| err.into_status_body())?;
    users::claim_unowned_rows(&state.db, &user.id)
        .await
        .map_err(|err| err.into_status_body())?;
    user_settings::upsert(&state.db, &user.id, &settings)
        .await
        .map_err(|err| err.into_status_body())?;
    session
        .insert("user_id", &user.id)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(AuthUser {
        id: user.id,
        username: user.username,
        role: user.role,
    }))
}

pub async fn require_session(
    session: Session,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let user: Option<String> = session.get("user_id").await.unwrap_or(None);
    if user.is_some() {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub async fn current_user(
    state: &AppState,
    session: &Session,
) -> Result<AuthUser, (StatusCode, String)> {
    let user_id: Option<String> = session
        .get("user_id")
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let user_id =
        user_id.ok_or_else(|| (StatusCode::UNAUTHORIZED, "Login required".to_string()))?;
    let user = users::find_by_id(&state.db, &user_id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(AuthUser {
        id: user.id,
        username: user.username,
        role: user.role,
    })
}

fn new_user_id() -> String {
    let raw: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();
    format!("usr_{raw}")
}
